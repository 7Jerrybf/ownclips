// Automatic recording: waits for Valorant to enter a match (gamestate.rs),
// records it with the built-in recorder, detects your kills and deaths on
// the recorded frames, and stops when the match is over. Each recording
// gets a `<recording>.events.json` next to it with the match info and every
// event's time in the video; it is rewritten after each event so a crash
// loses at most the video's ending. Highlights are just ranges around the
// kill events (the app computes them); nothing is cut automatically.
//
// `AutoRecorder::tick` drives everything, so the same logic can run from
// the command line or from the app's background thread. `reanalyze` runs
// the detector again over a finished recording.
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::capture;
use crate::clip;
use crate::config;
use crate::gamestate::{self, GameEvent, LogWatcher};
use crate::killfeed::{self, RowCounter};
use crate::recorder::{self, Recorder};
use crate::storage;

/// Keep recording this long after the match is decided (final kill, the
/// victory banner) before stopping.
const END_TAIL: Duration = Duration::from_secs(6);
/// Shorter recordings without any event are discarded as false starts.
const MIN_MATCH_SECONDS: f64 = 20.0;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Kill,
    Death,
    Assist,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MatchEvent {
    /// Seconds into the recording.
    pub t: f64,
    pub kind: EventKind,
}

/// Contents of `<recording>.events.json`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MatchInfo {
    pub map: String,
    pub mode: String,
    /// Local time the recording started, "YYYY-MM-DD HH:MM:SS".
    pub started_at: String,
    /// Recording length in seconds, once finished.
    pub duration: Option<f64>,
    pub events: Vec<MatchEvent>,
    /// Never deleted by the recordings size limit.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep: bool,
}

impl MatchInfo {
    pub fn path_for(video: &Path) -> PathBuf {
        video.with_extension("events.json")
    }

    pub fn load(video: &Path) -> Option<Self> {
        serde_json::from_str(&fs::read_to_string(Self::path_for(video)).ok()?).ok()
    }

    pub(crate) fn save(&self, video: &Path) -> Result<()> {
        fs::write(Self::path_for(video), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// What the auto recorder reports as it goes.
#[derive(Debug, Clone)]
pub enum Notice {
    Started { video: PathBuf, map: String, mode: String },
    Event { kind: EventKind, t: f64 },
    /// Recording stopped; the final file is being put together.
    Processing { video: PathBuf },
    /// A recording left unfinished by a crash was completed.
    Recovered { video: PathBuf },
    Stopped { video: PathBuf },
    /// The recordings size limit deleted the oldest recordings.
    Cleaned { deleted: usize, freed: u64 },
    Skipped { reason: String },
    Error(String),
}

struct ActiveMatch {
    video: PathBuf,
    info: MatchInfo,
    recorder: Recorder,
    kills: RowCounter,
    deaths: RowCounter,
    assists: RowCounter,
    last_generation: u64,
    stop_at: Option<Instant>,
}

pub struct AutoRecorder {
    log: LogWatcher,
    current: Option<ActiveMatch>,
    /// Background work (muxing, clipping) reports back through here.
    done_tx: Sender<Notice>,
    done_rx: Receiver<Notice>,
    jobs: Vec<JoinHandle<()>>,
}

impl AutoRecorder {
    pub fn new() -> Result<Self> {
        let (done_tx, done_rx) = mpsc::channel();
        let mut auto = Self { log: LogWatcher::new()?, current: None, done_tx, done_rx, jobs: Vec::new() };
        auto.recover();
        Ok(auto)
    }

    /// Finishes recordings a crash left behind.
    fn recover(&mut self) {
        let Ok(dir) = config::load().and_then(|c| c.recordings_dir()) else { return };
        let tx = self.done_tx.clone();
        self.jobs.push(thread::spawn(move || {
            for result in recorder::recover_unfinished(&dir) {
                let _ = tx.send(match result {
                    Ok(video) => Notice::Recovered { video },
                    Err(e) => Notice::Error(format!("救回未完成的錄影失敗: {e:#}")),
                });
            }
            send_cleanup(&tx);
        }));
    }

    pub fn is_recording(&self) -> bool {
        self.current.is_some()
    }

    /// Advances the state machine; call a few times per second.
    pub fn tick(&mut self) -> Vec<Notice> {
        let mut notices: Vec<Notice> = self.done_rx.try_iter().collect();
        self.jobs.retain(|j| !j.is_finished());
        for event in self.log.poll() {
            match event {
                GameEvent::MatchStarted { map, mode } => {
                    if self.current.is_some() {
                        continue;
                    }
                    if gamestate::is_practice(&mode) {
                        notices.push(Notice::Skipped { reason: format!("{mode}不錄影") });
                        continue;
                    }
                    match self.start_match(map.clone(), mode.clone()) {
                        Ok(video) => notices.push(Notice::Started { video, map, mode }),
                        Err(e) => notices.push(Notice::Error(format!("開始錄影失敗: {e:#}"))),
                    }
                }
                GameEvent::MatchEnded => {
                    if let Some(m) = &mut self.current {
                        m.stop_at.get_or_insert(Instant::now() + END_TAIL);
                    }
                }
                GameEvent::LeftMatch => notices.extend(self.finish_match()),
            }
        }

        if let Some(m) = &mut self.current {
            notices.extend(detect(m));
            if let Some(e) = m.recorder.error() {
                notices.push(Notice::Error(format!("錄影發生錯誤: {e}")));
                notices.extend(self.finish_match());
            } else if m.stop_at.is_some_and(|t| Instant::now() >= t) {
                notices.extend(self.finish_match());
            }
        }
        notices
    }

    fn start_match(&mut self, map: String, mode: String) -> Result<PathBuf> {
        let cfg = config::load()?;
        let window = capture::find_valorant_window()?;
        let dir = cfg.recordings_dir()?;
        fs::create_dir_all(&dir)?;
        let now = chrono::Local::now();
        let video = dir.join(format!("{}.mp4", now.format("%Y-%m-%d %H-%M-%S")));
        let recorder = Recorder::start(window, &video, cfg.record_settings())?;
        let info = MatchInfo {
            map,
            mode,
            started_at: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            duration: None,
            events: Vec::new(),
            keep: false,
        };
        info.save(&video)?;
        self.current = Some(ActiveMatch {
            video: video.clone(),
            info,
            recorder,
            kills: RowCounter::default(),
            deaths: RowCounter::default(),
            assists: RowCounter::default(),
            last_generation: 0,
            stop_at: None,
        });
        Ok(video)
    }

    fn finish_match(&mut self) -> Vec<Notice> {
        let Some(mut m) = self.current.take() else { return Vec::new() };
        m.info.duration = m.recorder.elapsed();
        let stopped = match m.recorder.stop() {
            Ok(s) => s,
            Err(e) => return vec![Notice::Error(format!("{e:#}"))],
        };
        // A few seconds with nothing in it is a false start, not a match.
        if m.info.duration.is_none_or(|d| d < MIN_MATCH_SECONDS) && m.info.events.is_empty() {
            stopped.discard();
            let _ = fs::remove_file(MatchInfo::path_for(&m.video));
            return vec![Notice::Skipped { reason: "錄影太短,已捨棄".to_string() }];
        }
        let _ = m.info.save(&m.video);

        // Muxing a long match takes a while; don't hold up the next one.
        let tx = self.done_tx.clone();
        let video = m.video.clone();
        self.jobs.push(thread::spawn(move || {
            let _ = tx.send(match stopped.finish() {
                Ok(_) => Notice::Stopped { video },
                Err(e) => Notice::Error(format!("{e:#}")),
            });
            send_cleanup(&tx);
        }));
        vec![Notice::Processing { video: m.video }]
    }

    /// Stops a recording in progress and waits for background work (e.g.
    /// the app is closing), so files are left complete.
    pub fn shutdown(&mut self) -> Vec<Notice> {
        let mut notices = self.finish_match();
        for j in self.jobs.drain(..) {
            let _ = j.join();
        }
        notices.extend(self.done_rx.try_iter());
        notices
    }
}

/// Applies the recordings size limit, reporting what it deleted.
fn send_cleanup(tx: &Sender<Notice>) {
    match storage::enforce() {
        Ok((0, _)) => {}
        Ok((deleted, freed)) => {
            let _ = tx.send(Notice::Cleaned { deleted, freed });
        }
        Err(e) => {
            let _ = tx.send(Notice::Error(format!("清理舊錄影失敗: {e:#}")));
        }
    }
}

fn detect(m: &mut ActiveMatch) -> Vec<Notice> {
    // Without a killfeed calibration the recorder captures no detection
    // frames and the match is only recorded.
    let sink = m.recorder.frames();
    let frame = {
        let s = sink.lock().unwrap();
        if s.generation == m.last_generation {
            return Vec::new();
        }
        m.last_generation = s.generation;
        s.latest.clone()
    };
    let (Some(frame), Some(t)) = (frame, m.recorder.elapsed()) else { return Vec::new() };
    let Some(crop) = frame.to_rgb() else { return Vec::new() };

    let scan = killfeed::scan(&crop, killfeed::ROW_HEIGHT_FRAC * frame.source_height as f32);
    let mut notices = Vec::new();
    let found = [
        (EventKind::Kill, m.kills.update(&scan.kills, t)),
        (EventKind::Death, m.deaths.update(&scan.deaths, t)),
        (EventKind::Assist, m.assists.update(&scan.assists, t)),
    ];
    for (kind, n) in found {
        for _ in 0..n {
            m.info.events.push(MatchEvent { t, kind });
            notices.push(Notice::Event { kind, t });
        }
    }
    if !notices.is_empty() {
        let _ = m.info.save(&m.video);
        if let Ok(dir) = config::detect_samples_dir() {
            let _ = crop.save(dir.join(format!("{} {t:07.1}s.png", stem(&m.video))));
        }
    }
    notices
}

/// Frames per second read when re-analyzing (the live detector sees 4).
const REANALYZE_FPS: u32 = 8;

/// Runs the current killfeed detector over a finished recording and
/// rewrites its events. Kills logged live (seen uncompressed) that the new
/// pass doesn't find are kept; deaths and assists come from the new pass
/// only. The events file as first recorded is kept as `.events.orig.json`.
pub fn reanalyze(video: &Path, mut progress: impl FnMut(f64)) -> Result<MatchInfo> {
    let mut info = MatchInfo::load(video).ok_or_else(|| anyhow!("找不到這場錄影的事件檔"))?;
    // The live events to merge with are always the ones first recorded.
    let original = video.with_extension("events.orig.json");
    let live: Vec<MatchEvent> = fs::read_to_string(&original)
        .ok()
        .and_then(|s| serde_json::from_str::<MatchInfo>(&s).ok())
        .map_or_else(|| info.events.clone(), |o| o.events);
    let region = config::load()?.killfeed_region.ok_or_else(|| anyhow!("還沒校準 killfeed 區域"))?;
    let (width, height) = video_size(video)?;
    let (x0, y0, x1, y1) = capture::region_px(config::detect_region(region), width, height);
    let (w, h) = (x1 - x0, y1 - y0);

    let mut ffmpeg = clip::ffmpeg()?
        // GPU decoding where available: a long 1440p match takes minutes on the CPU.
        .args(["-v", "error", "-hwaccel", "auto", "-i"])
        .arg(video)
        .args(["-map", "0:v:0", "-vf", &format!("fps={REANALYZE_FPS},crop={w}:{h}:{x0}:{y0}")])
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut frames = ffmpeg.stdout.take().unwrap();
    let mut buf = vec![0u8; (w * h * 3) as usize];
    let row_h = killfeed::ROW_HEIGHT_FRAC * height as f32;
    let (mut kills, mut deaths, mut assists) = (RowCounter::default(), RowCounter::default(), RowCounter::default());
    let mut events = Vec::new();
    let duration = info.duration.unwrap_or(0.0);
    let mut i = 0u64;
    while frames.read_exact(&mut buf).is_ok() {
        let t = i as f64 / REANALYZE_FPS as f64;
        i += 1;
        let Some(img) = image::RgbImage::from_raw(w, h, buf.clone()) else { break };
        let scan = killfeed::scan(&img, row_h);
        let found = [
            (EventKind::Kill, kills.update(&scan.kills, t)),
            (EventKind::Death, deaths.update(&scan.deaths, t)),
            (EventKind::Assist, assists.update(&scan.assists, t)),
        ];
        for (kind, n) in found {
            events.extend((0..n).map(|_| MatchEvent { t, kind }));
        }
        if duration > 0.0 && i % (REANALYZE_FPS as u64 * 5) == 0 {
            progress((t / duration).min(1.0));
        }
    }
    if !ffmpeg.wait()?.success() {
        return Err(anyhow!("讀取錄影失敗: {}", video.display()));
    }

    // Each live kill matches at most one new kill (the new pass tends to
    // see a row a few seconds later), so a double kill seen live survives a
    // pass that only caught one of them.
    let mut matched = vec![false; events.len()];
    let mut unmatched = Vec::new();
    for live in live.iter().filter(|e| e.kind == EventKind::Kill) {
        let near = (0..matched.len())
            .filter(|&j| !matched[j] && events[j].kind == EventKind::Kill && (events[j].t - live.t).abs() < 4.0)
            .min_by(|&a, &b| (events[a].t - live.t).abs().total_cmp(&(events[b].t - live.t).abs()));
        match near {
            Some(j) => matched[j] = true,
            None => unmatched.push(live.clone()),
        }
    }
    events.extend(unmatched);
    events.sort_by(|a, b| a.t.total_cmp(&b.t));

    if !original.exists() {
        fs::copy(MatchInfo::path_for(video), &original)?;
    }
    info.events = events;
    info.save(video)?;
    progress(1.0);
    Ok(info)
}

/// Width and height of a video's first video stream, from ffmpeg's report.
fn video_size(video: &Path) -> Result<(u32, u32)> {
    let out = clip::ffmpeg()?.arg("-hide_banner").arg("-i").arg(video).output()?;
    let report = String::from_utf8_lossy(&out.stderr);
    report
        .lines()
        .filter(|l| l.contains("Video:"))
        .flat_map(|l| l.split([' ', ',']))
        .find_map(|tok| {
            let (w, h) = tok.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?)).filter(|&(w, h): &(u32, u32)| w >= 16 && h >= 16)
        })
        .ok_or_else(|| anyhow!("讀不到錄影的解析度: {}", video.display()))
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Command-line front end: runs the auto recorder until Ctrl+C.
pub fn run() -> Result<()> {
    let cfg = config::load()?;
    if cfg.killfeed_region.is_none() {
        return Err(anyhow!("還沒校準 killfeed 區域,請先執行校準。"));
    }
    println!("錄影資料夾: {}", cfg.recordings_dir()?.display());
    let mut auto = AutoRecorder::new()?;
    // Ctrl+C must still finalize the recording: an unfinished MP4 is unplayable.
    let quit = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let quit2 = quit.clone();
    ctrlc::set_handler(move || quit2.store(true, std::sync::atomic::Ordering::Relaxed))?;
    println!("等待 Valorant 對局開始...(會自動錄影,對局結束自動停止。按 Ctrl+C 結束)");
    loop {
        let quitting = quit.load(std::sync::atomic::Ordering::Relaxed);
        let notices = if quitting { auto.shutdown() } else { auto.tick() };
        for n in notices {
            match n {
                Notice::Started { video, map, mode } => println!("對局開始 [{mode} / {map}],錄影中: {}", video.display()),
                Notice::Event { kind, t } => {
                    let what = match kind {
                        EventKind::Kill => "擊殺",
                        EventKind::Death => "死亡",
                        EventKind::Assist => "助攻",
                    };
                    println!("  {what} @ {:02}:{:04.1}", (t / 60.0) as u64, t % 60.0);
                }
                Notice::Stopped { video } => {
                    println!("對局結束,錄影已存檔: {}", video.display());
                    println!("等待下一場...");
                }
                Notice::Processing { .. } => println!("對局結束,正在合成錄影檔..."),
                Notice::Recovered { video } => println!("已救回未完成的錄影: {}", video.display()),
                Notice::Cleaned { deleted, freed } => {
                    println!("超過容量上限,已刪除最舊的 {deleted} 場錄影({:.1} GB)", freed as f64 / storage::GB as f64)
                }
                Notice::Skipped { reason } => println!("{reason}"),
                Notice::Error(e) => eprintln!("錯誤: {e}"),
            }
        }
        if quitting {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(if auto.is_recording() { 150 } else { 1000 }));
    }
}
