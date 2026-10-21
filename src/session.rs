// Automatic recording: waits for Valorant to enter a match (gamestate.rs),
// records it with the built-in recorder, detects your kills and deaths on
// the recorded frames, and stops when the match is over. Each recording
// gets a `<recording>.events.json` next to it with the match info and every
// event's time in the video; it is rewritten after each event so a crash
// loses at most the video's ending. Clips are cut automatically afterwards.
//
// `AutoRecorder::tick` drives everything, so the same logic can run from
// the command line or from the app's background thread.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::capture;
use crate::clip;
use crate::config::{self, RegionPct};
use crate::gamestate::{self, GameEvent, LogWatcher};
use crate::killfeed::{self, RowCounter};
use crate::recorder::Recorder;

/// Keep recording this long after the match is decided (final kill, the
/// victory banner) before stopping.
const END_TAIL: Duration = Duration::from_secs(6);
/// Min gap between saved "maybe an assist" frames.
const ASSIST_SAMPLE_GAP: Duration = Duration::from_secs(2);
const STRAY_YELLOW_MIN: usize = 40;
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
}

impl MatchInfo {
    pub fn path_for(video: &Path) -> PathBuf {
        video.with_extension("events.json")
    }

    pub fn load(video: &Path) -> Option<Self> {
        serde_json::from_str(&fs::read_to_string(Self::path_for(video)).ok()?).ok()
    }

    fn save(&self, video: &Path) -> Result<()> {
        fs::write(Self::path_for(video), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// What the auto recorder reports as it goes.
#[derive(Debug, Clone)]
pub enum Notice {
    Started { video: PathBuf, map: String, mode: String },
    Event { kind: EventKind, t: f64 },
    Stopped { video: PathBuf, clips: usize },
    Skipped { reason: String },
    Error(String),
}

struct ActiveMatch {
    video: PathBuf,
    info: MatchInfo,
    recorder: Recorder,
    /// Killfeed region; without a calibration the match is only recorded.
    region: Option<RegionPct>,
    kills: RowCounter,
    deaths: RowCounter,
    last_generation: u64,
    stop_at: Option<Instant>,
    last_assist_sample: Option<Instant>,
}

pub struct AutoRecorder {
    log: LogWatcher,
    current: Option<ActiveMatch>,
}

impl AutoRecorder {
    pub fn new() -> Result<Self> {
        Ok(Self { log: LogWatcher::new()?, current: None })
    }

    pub fn is_recording(&self) -> bool {
        self.current.is_some()
    }

    /// Advances the state machine; call a few times per second.
    pub fn tick(&mut self) -> Vec<Notice> {
        let mut notices = Vec::new();
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
        };
        info.save(&video)?;
        self.current = Some(ActiveMatch {
            video: video.clone(),
            info,
            recorder,
            region: cfg.killfeed_region,
            kills: RowCounter::default(),
            deaths: RowCounter::default(),
            last_generation: 0,
            stop_at: None,
            last_assist_sample: None,
        });
        Ok(video)
    }

    fn finish_match(&mut self) -> Vec<Notice> {
        let Some(mut m) = self.current.take() else { return Vec::new() };
        m.info.duration = m.recorder.elapsed();
        let mut notices = Vec::new();
        if let Err(e) = m.recorder.stop() {
            notices.push(Notice::Error(format!("{e:#}")));
            return notices;
        }
        // A few seconds with nothing in it is a false start, not a match.
        if m.info.duration.is_none_or(|d| d < MIN_MATCH_SECONDS) && m.info.events.is_empty() {
            let _ = fs::remove_file(&m.video);
            let _ = fs::remove_file(MatchInfo::path_for(&m.video));
            notices.push(Notice::Skipped { reason: "錄影太短,已捨棄".to_string() });
            return notices;
        }
        let _ = m.info.save(&m.video);
        let clips = match config::load() {
            Ok(cfg) if cfg.auto_clip => clip::clip_match(&m.video, &cfg).unwrap_or_else(|e| {
                notices.push(Notice::Error(format!("自動裁切失敗: {e:#}")));
                0
            }),
            _ => 0,
        };
        notices.push(Notice::Stopped { video: m.video, clips });
        notices
    }

    /// Stops a recording in progress (e.g. the app is closing).
    pub fn shutdown(&mut self) -> Vec<Notice> {
        self.finish_match()
    }
}

fn detect(m: &mut ActiveMatch) -> Vec<Notice> {
    let Some(region) = m.region else { return Vec::new() };
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
    let Ok(crop) = frame.crop_rgb(region) else { return Vec::new() };

    let scan = killfeed::scan(&crop, killfeed::ROW_HEIGHT_FRAC * frame.height as f32);
    let mut notices = Vec::new();
    let found = [
        (EventKind::Kill, m.kills.update(scan.kills)),
        (EventKind::Death, m.deaths.update(scan.deaths)),
    ];
    for (kind, n) in found {
        for _ in 0..n {
            m.info.events.push(MatchEvent { t, kind });
            notices.push(Notice::Event { kind, t });
        }
    }
    if notices.is_empty() {
        if scan.stray_yellow >= STRAY_YELLOW_MIN
            && m.last_assist_sample.is_none_or(|s| s.elapsed() >= ASSIST_SAMPLE_GAP)
        {
            m.last_assist_sample = Some(Instant::now());
            if let Ok(dir) = config::assist_samples_dir() {
                let _ = crop.save(dir.join(format!("{} {t:07.1}s.png", stem(&m.video))));
            }
        }
    } else {
        let _ = m.info.save(&m.video);
        if let Ok(dir) = config::detect_samples_dir() {
            let _ = crop.save(dir.join(format!("{} {t:07.1}s.png", stem(&m.video))));
        }
    }
    notices
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
    println!("等待 Valorant 對局開始...(會自動錄影,對局結束自動停止並裁切片段。按 Ctrl+C 結束)");
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
                Notice::Stopped { video, clips } => {
                    println!("對局結束,錄影已存檔: {}(自動裁出 {clips} 個片段)", video.display());
                    println!("等待下一場...");
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
