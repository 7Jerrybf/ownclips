// Built-in recorder (replaces OBS).
//
// Video: ffmpeg captures the Valorant window itself (gfxcapture, Windows
// Graphics Capture) and encodes it on the GPU (NVENC when available),
// keeping the capture's own frame timing (no duplicated or dropped frames).
// It writes MKV, which stays playable if anything crashes mid-match.
//
// Audio: recorded separately by audio.rs, aligned to the performance
// counter. When recording stops, video and audio are muxed into the final
// MP4. To line them up we need the performance-counter time of the
// video's first frame; ffmpeg reports how far into the video it is
// (out_time) ten times a second, and "now - out_time" can only overshoot
// the true start (by processing delay), so the minimum over the recording
// is a tight estimate.
//
// Once frames flow, ffmpeg gets a high GPU scheduling priority so a game
// saturating the GPU can't starve the capture (priority.rs).
//
// A separate low-rate capture (4 fps) of just the killfeed region feeds the
// killfeed detector.
use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use windows_capture::window::Window;

use crate::audio::{self, AudioMode, AudioRecorder};
use crate::capture::{self, Control, SharedSink};
use crate::clip;
use crate::config::RegionPct;
use crate::{priority, qpc};

const DETECT_FRAME_INTERVAL: Duration = Duration::from_millis(250);
/// How much later than the true first frame the progress-based estimate
/// lands: a fixed part plus a few frames of pipeline queueing. Measured
/// with a flash+beep test page at 30 and 60 fps (residual error ~15 ms).
fn video_start_bias_ticks(fps: u32) -> i64 {
    let secs = 0.027 + 2.7 / fps.max(1) as f64;
    (secs * qpc::TICKS_PER_SEC) as i64
}

#[derive(Clone, Copy)]
pub struct RecordSettings {
    pub fps: u32,
    pub bitrate_mbps: u32,
    pub audio: AudioMode,
    /// Window region the killfeed detector watches; None captures no
    /// detection frames.
    pub detect_region: Option<RegionPct>,
}

/// Files of one recording. While recording: `<stem>.mkv` (video),
/// `<stem>.audio.pcm` and `<stem>.rec.json`; afterwards just `<stem>.mp4`.
#[derive(Clone)]
struct Files {
    mp4: PathBuf,
    mkv: PathBuf,
    pcm: PathBuf,
    meta: PathBuf,
}

impl Files {
    fn for_output(mp4: &Path) -> Self {
        Self {
            mp4: mp4.to_path_buf(),
            mkv: mp4.with_extension("mkv"),
            pcm: mp4.with_extension("audio.pcm"),
            meta: mp4.with_extension("rec.json"),
        }
    }
}

/// Timing needed to mux the audio, saved so a crashed recording can still
/// be finished later.
#[derive(Serialize, Deserialize, Default, Clone, Copy)]
struct RecMeta {
    audio_start: u64,
    video_start: Option<u64>,
    #[serde(default)]
    fps: u32,
}

pub struct Recorder {
    files: Files,
    ffmpeg: Child,
    stdin: Option<ChildStdin>,
    audio: Option<AudioRecorder>,
    audio_start: u64,
    video_start: Arc<Mutex<Option<u64>>>,
    fps: u32,
    detect: Option<Control>,
    sink: SharedSink,
    log: Arc<Mutex<VecDeque<String>>>,
}

impl Recorder {
    /// Starts recording `window`; the finished file will be `mp4`.
    pub fn start(window: Window, mp4: &Path, settings: RecordSettings) -> Result<Self> {
        let files = Files::for_output(mp4);
        let encoder = pick_encoder()?;
        let hwnd = window.as_raw_hwnd() as usize;
        let pid = window.process_id().map_err(|e| anyhow!("讀取遊戲程序 ID 失敗: {e}"))?;

        let audio_start = qpc::now();
        let audio = AudioRecorder::start(settings.audio, pid, audio_start, &files.pcm)?;
        save_meta(&files, RecMeta { audio_start, video_start: None, fps: settings.fps });

        // Capture at the display's own pace and let the fps filter pick the
        // frame nearest each slot of a steady grid. Throttling the capture
        // itself to the target rate instead lands frames on multiples of the
        // refresh interval (e.g. 6.06 ms at 165 Hz), giving uneven spacing.
        let filter = format!(
            "gfxcapture=hwnd={hwnd}:max_framerate=240:capture_cursor=0:width=-2:height=-2,fps={}{}",
            settings.fps, encoder.filter
        );
        let mut cmd = clip::ffmpeg()?;
        cmd.args(["-hide_banner", "-loglevel", "warning", "-stats_period", "0.1", "-progress", "pipe:2"])
            .args(["-filter_complex", &filter, "-fps_mode", "passthrough"])
            .args((encoder.args)(settings.fps, settings.bitrate_mbps))
            .args(["-f", "matroska", "-y"])
            .arg(&files.mkv)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut ffmpeg = cmd.spawn().context("無法啟動 ffmpeg")?;

        let video_start = Arc::new(Mutex::new(None));
        let log = Arc::new(Mutex::new(VecDeque::new()));
        let stderr = ffmpeg.stderr.take().unwrap();
        let (vs, lg, ffmpeg_pid) = (video_start.clone(), log.clone(), ffmpeg.id());
        thread::spawn(move || read_progress(stderr, ffmpeg_pid, &vs, &lg));

        let (detect, sink) = match settings.detect_region {
            Some(region) => {
                let detect_window = Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void);
                let (c, s) = capture::start_capture_every(detect_window, Some(DETECT_FRAME_INTERVAL), Some(region))?;
                (Some(c), s)
            }
            None => (None, SharedSink::default()),
        };

        Ok(Self {
            files,
            stdin: ffmpeg.stdin.take(),
            ffmpeg,
            audio,
            audio_start,
            video_start,
            fps: settings.fps,
            detect,
            sink,
            log,
        })
    }

    /// Latest frames for the detector: the `detect_region` of the window.
    pub fn frames(&self) -> SharedSink {
        self.sink.clone()
    }

    /// Seconds since the video's first frame: the position in the video.
    pub fn elapsed(&self) -> Option<f64> {
        let start = (*self.video_start.lock().unwrap())? as i64 - video_start_bias_ticks(self.fps);
        Some((qpc::now() as i64 - start).max(0) as f64 / qpc::TICKS_PER_SEC)
    }

    /// Why the recording failed, if ffmpeg has quit on its own.
    pub fn error(&mut self) -> Option<String> {
        match self.ffmpeg.try_wait() {
            Ok(Some(status)) => {
                let tail: Vec<String> = self.log.lock().unwrap().iter().cloned().collect();
                Some(format!("ffmpeg 結束了({status}): {}", tail.join(" / ")))
            }
            _ => None,
        }
    }

    /// Stops recording. The returned recording still has to be finished
    /// (muxed) with [`Stopped::finish`], which takes a while.
    pub fn stop(mut self) -> Result<Stopped> {
        if let Some(c) = self.detect.take() {
            let _ = c.stop();
        }
        // "q" asks ffmpeg to finish the file cleanly.
        if let Some(mut stdin) = self.stdin.take() {
            let _ = stdin.write_all(b"q\n");
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            match self.ffmpeg.try_wait() {
                Ok(Some(_)) => break,
                _ if Instant::now() > deadline => {
                    let _ = self.ffmpeg.kill();
                    let _ = self.ffmpeg.wait();
                    break;
                }
                _ => thread::sleep(Duration::from_millis(50)),
            }
        }
        if let Some(audio) = self.audio.take() {
            audio.stop()?;
        }
        let meta = RecMeta { audio_start: self.audio_start, video_start: *self.video_start.lock().unwrap(), fps: self.fps };
        save_meta(&self.files, meta);
        Ok(Stopped { files: self.files, meta })
    }
}

/// A recording that has stopped but not yet been muxed into its MP4.
pub struct Stopped {
    files: Files,
    meta: RecMeta,
}

impl Stopped {
    /// Muxes video and audio into the final MP4 and removes the temporary
    /// files; returns the MP4's path.
    pub fn finish(self) -> Result<PathBuf> {
        finish_files(&self.files, self.meta)
    }

    /// Deletes everything (a false start).
    pub fn discard(self) {
        for p in [&self.files.mkv, &self.files.pcm, &self.files.meta, &self.files.mp4] {
            let _ = fs::remove_file(p);
        }
    }
}

/// Finishes recordings left unfinished by a crash; returns their MP4s.
pub fn recover_unfinished(dir: &Path) -> Vec<Result<PathBuf>> {
    let metas: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    metas
        .into_iter()
        .filter(|p| p.to_string_lossy().ends_with(".rec.json"))
        .filter_map(|meta_path| {
            let name = meta_path.file_name()?.to_string_lossy().into_owned();
            let mp4 = meta_path.with_file_name(name.strip_suffix(".rec.json")?.to_string() + ".mp4");
            let files = Files::for_output(&mp4);
            let meta: RecMeta = serde_json::from_str(&fs::read_to_string(&meta_path).ok()?).ok()?;
            if !files.mkv.exists() {
                let _ = fs::remove_file(&meta_path);
                return None;
            }
            Some(finish_files(&files, meta))
        })
        .collect()
}

fn save_meta(files: &Files, meta: RecMeta) {
    if let Ok(json) = serde_json::to_string(&meta) {
        let _ = fs::write(&files.meta, json);
    }
}

fn finish_files(files: &Files, meta: RecMeta) -> Result<PathBuf> {
    let has_audio = fs::metadata(&files.pcm).is_ok_and(|m| m.len() > 0);
    let mut cmd = clip::ffmpeg()?;
    cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-i"]).arg(&files.mkv);
    if has_audio {
        // Audio sample 0 is at `audio_start`, video frame 0 at `video_start`.
        let video_start =
            meta.video_start.map_or(meta.audio_start as i64, |v| v as i64 - video_start_bias_ticks(meta.fps.max(30)));
        let skip = (video_start - meta.audio_start as i64) as f64 / qpc::TICKS_PER_SEC;
        if skip >= 0.0 {
            cmd.args(["-ss", &format!("{skip:.4}")]);
        } else {
            cmd.args(["-itsoffset", &format!("{:.4}", -skip)]);
        }
        cmd.args(["-f", "s16le", "-ar", &audio::SAMPLE_RATE.to_string(), "-ac", &audio::CHANNELS.to_string(), "-i"])
            .arg(&files.pcm)
            .args(["-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k"]);
    } else {
        cmd.args(["-c", "copy"]);
    }
    let status = cmd.args(["-movflags", "+faststart"]).arg(&files.mp4).status()?;
    if !status.success() {
        return Err(anyhow!("合成錄影檔失敗,原始檔保留在: {}", files.mkv.display()));
    }
    for p in [&files.mkv, &files.pcm, &files.meta] {
        let _ = fs::remove_file(p);
    }
    Ok(files.mp4.clone())
}

fn read_progress(
    stderr: std::process::ChildStderr,
    ffmpeg_pid: u32,
    video_start: &Mutex<Option<u64>>,
    log: &Mutex<VecDeque<String>>,
) {
    let mut boosted = false;
    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
        let now = qpc::now();
        if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<i64>().ok()) {
            if us > 0 {
                // Frames are flowing, so ffmpeg has its GPU device by now.
                if !boosted {
                    boosted = true;
                    if !priority::boost(ffmpeg_pid) {
                        log.lock().unwrap().push_back("無法提高錄影的 GPU 優先權".into());
                    }
                }
                let estimate = now.saturating_sub(us as u64 * 10);
                let mut vs = video_start.lock().unwrap();
                *vs = Some(vs.map_or(estimate, |v| v.min(estimate)));
            }
        } else if !line.contains('=') {
            let mut log = log.lock().unwrap();
            log.push_back(line);
            if log.len() > 8 {
                log.pop_front();
            }
        }
    }
}

struct Encoder {
    name: &'static str,
    /// Appended to the capture filter (e.g. download + colour conversion).
    filter: &'static str,
    args: fn(u32, u32) -> Vec<String>,
}

const SW_CONVERT: &str = ",hwdownload,format=bgra,scale=out_color_matrix=bt709:out_range=tv,format=nv12";

fn rate_args(fps: u32, mbps: u32) -> Vec<String> {
    vec![
        "-b:v".into(),
        format!("{mbps}M"),
        "-maxrate".into(),
        format!("{}M", mbps * 3 / 2),
        "-bufsize".into(),
        format!("{}M", mbps * 2),
        "-g".into(),
        (fps * 2).to_string(),
    ]
}

fn bt709_tags() -> Vec<String> {
    ["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv"]
        .map(String::from)
        .to_vec()
}

const ENCODERS: [Encoder; 4] = [
    // NVENC takes the captured GPU frames directly and converts BGRA to
    // limited-range BT.601 YUV itself (measured), but would tag the stream
    // as RGB: setparams (container tags) and h264_metadata (bitstream tags)
    // label it correctly.
    Encoder {
        name: "h264_nvenc",
        filter: ",setparams=range=tv:color_primaries=bt709:color_trc=bt709:colorspace=smpte170m",
        args: |fps, mbps| {
            // "-delay 0" hands each packet out as soon as it is encoded, so
            // the progress-based start estimate isn't skewed by buffering.
            let mut a: Vec<String> = ["-c:v", "h264_nvenc", "-preset", "p5", "-tune", "hq", "-rc", "vbr", "-delay", "0"]
                .map(String::from)
                .to_vec();
            a.extend(rate_args(fps, mbps));
            a.extend(
                [
                    "-bsf:v",
                    "h264_metadata=video_full_range_flag=0:colour_primaries=1:transfer_characteristics=1:matrix_coefficients=6",
                ]
                .map(String::from),
            );
            a
        },
    },
    Encoder {
        name: "h264_amf",
        filter: SW_CONVERT,
        args: |fps, mbps| {
            let mut a: Vec<String> = ["-c:v", "h264_amf", "-quality", "balanced", "-rc", "vbr_peak"].map(String::from).to_vec();
            a.extend(rate_args(fps, mbps));
            a.extend(bt709_tags());
            a
        },
    },
    Encoder {
        name: "h264_qsv",
        filter: SW_CONVERT,
        args: |fps, mbps| {
            let mut a: Vec<String> = ["-c:v", "h264_qsv", "-preset", "medium"].map(String::from).to_vec();
            a.extend(rate_args(fps, mbps));
            a.extend(bt709_tags());
            a
        },
    },
    Encoder {
        name: "libx264",
        filter: SW_CONVERT,
        args: |fps, mbps| {
            let mut a: Vec<String> = ["-c:v", "libx264", "-preset", "veryfast"].map(String::from).to_vec();
            a.extend(rate_args(fps, mbps));
            a.extend(bt709_tags());
            a
        },
    },
];

/// The first encoder that actually works on this PC (an encoder can be
/// compiled into ffmpeg without the matching GPU being present).
fn pick_encoder() -> Result<&'static Encoder> {
    static PICKED: OnceLock<Option<usize>> = OnceLock::new();
    let idx = PICKED.get_or_init(|| {
        ENCODERS.iter().position(|e| {
            clip::ffmpeg().is_ok_and(|mut c| {
                c.args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=s=640x360:d=0.2"])
                    .args(["-frames:v", "3", "-pix_fmt", "nv12", "-c:v", e.name, "-f", "null", "-"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .is_ok_and(|s| s.success())
            })
        })
    });
    idx.map(|i| &ENCODERS[i]).ok_or_else(|| anyhow!("找不到可用的影片編碼器(ffmpeg 是否已安裝?)"))
}

/// Which encoder recordings use, for display.
pub fn encoder_name() -> Option<&'static str> {
    pick_encoder().ok().map(|e| e.name)
}
