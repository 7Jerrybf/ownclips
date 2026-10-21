// OwnClips desktop app backend: runs the auto recorder in a background
// thread (reporting to the UI through events) and exposes the recordings,
// clips, settings and killfeed calibration to the web frontend.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs;
use std::io::Cursor;
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use base64::Engine as _;
use ownclips::clip;
use ownclips::config::{self, Config, RegionPct};
use ownclips::session::{AutoRecorder, EventKind, MatchInfo, Notice};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ---------- background auto recorder ----------

#[derive(Serialize, Clone, Default)]
struct RecorderStatus {
    /// "waiting" | "recording"
    state: String,
    map: Option<String>,
    mode: Option<String>,
    video: Option<String>,
    /// Unix ms when the current recording started.
    started_at: Option<i64>,
    kills: u32,
    deaths: u32,
    last_error: Option<String>,
}

#[derive(Serialize, Clone)]
struct Toast {
    kind: String,
    message: String,
}

struct Background {
    status: Arc<Mutex<RecorderStatus>>,
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

fn start_background(app: AppHandle) -> Background {
    let status = Arc::new(Mutex::new(RecorderStatus { state: "waiting".into(), ..Default::default() }));
    let stop = Arc::new(AtomicBool::new(false));
    let (status2, stop2) = (status.clone(), stop.clone());
    let thread = thread::spawn(move || {
        let mut auto = match AutoRecorder::new() {
            Ok(a) => a,
            Err(e) => {
                status2.lock().unwrap().last_error = Some(format!("{e:#}"));
                let _ = app.emit("recorder-status", status2.lock().unwrap().clone());
                return;
            }
        };
        loop {
            let stopping = stop2.load(Ordering::Relaxed);
            let notices = if stopping { auto.shutdown() } else { auto.tick() };
            for n in notices {
                apply_notice(&app, &status2, n);
            }
            if stopping {
                return;
            }
            thread::sleep(Duration::from_millis(if auto.is_recording() { 150 } else { 1000 }));
        }
    });
    Background { status, stop, thread: Mutex::new(Some(thread)) }
}

fn apply_notice(app: &AppHandle, status: &Mutex<RecorderStatus>, notice: Notice) {
    let toast = {
        let mut s = status.lock().unwrap();
        match notice {
            Notice::Started { video, map, mode } => {
                *s = RecorderStatus {
                    state: "recording".into(),
                    map: Some(map.clone()),
                    mode: Some(mode.clone()),
                    video: Some(video.to_string_lossy().into_owned()),
                    started_at: Some(now_ms()),
                    ..Default::default()
                };
                Some(Toast { kind: "start".into(), message: format!("對局開始,錄影中 · {mode} · {map}") })
            }
            Notice::Event { kind, .. } => {
                match kind {
                    EventKind::Kill => s.kills += 1,
                    EventKind::Death => s.deaths += 1,
                    EventKind::Assist => {}
                }
                None
            }
            Notice::Stopped { clips, .. } => {
                *s = RecorderStatus { state: "waiting".into(), ..Default::default() };
                let _ = app.emit("library-changed", ());
                Some(Toast { kind: "stop".into(), message: format!("對局結束,錄影已存檔,自動裁出 {clips} 個片段") })
            }
            Notice::Skipped { reason } => Some(Toast { kind: "info".into(), message: reason }),
            Notice::Error(e) => {
                s.last_error = Some(e.clone());
                Some(Toast { kind: "error".into(), message: e })
            }
        }
    };
    let _ = app.emit("recorder-status", status.lock().unwrap().clone());
    if let Some(t) = toast {
        let _ = app.emit("toast", t);
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[tauri::command]
fn get_status(bg: State<Background>) -> RecorderStatus {
    bg.status.lock().unwrap().clone()
}

// ---------- settings ----------

#[tauri::command]
fn get_config() -> CmdResult<Config> {
    config::load().map_err(err)
}

#[tauri::command]
fn save_config(cfg: Config) -> CmdResult<()> {
    config::save(&cfg).map(|_| ()).map_err(err)
}

#[tauri::command]
fn recordings_dir() -> CmdResult<String> {
    Ok(config::load().map_err(err)?.recordings_dir().map_err(err)?.to_string_lossy().into_owned())
}

#[tauri::command]
fn ffmpeg_available() -> bool {
    clip::ffmpeg_available()
}

// ---------- library ----------

#[derive(Serialize)]
struct MatchSummary {
    video: String,
    name: String,
    info: MatchInfo,
    size: u64,
    recording: bool,
}

#[derive(Serialize)]
struct ClipSummary {
    path: String,
    name: String,
    /// Recording the clip came from (its file stem).
    recording: String,
    /// Position in the recording, e.g. "03m27s".
    at: String,
    /// "3殺", "精選"...
    label: String,
    kills: Option<u32>,
    size: u64,
    modified: i64,
}

fn dir_files(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect())
        .unwrap_or_default()
}

#[tauri::command]
fn list_matches(bg: State<Background>) -> CmdResult<Vec<MatchSummary>> {
    let dir = config::load().map_err(err)?.recordings_dir().map_err(err)?;
    let current = bg.status.lock().unwrap().video.clone();
    let mut out: Vec<MatchSummary> = dir_files(&dir)
        .into_iter()
        .filter(|p| clip::is_video(p))
        .filter_map(|p| {
            let info = MatchInfo::load(&p)?;
            let video = p.to_string_lossy().into_owned();
            Some(MatchSummary {
                name: p.file_stem()?.to_string_lossy().into_owned(),
                size: fs::metadata(&p).map(|m| m.len()).unwrap_or(0),
                recording: current.as_deref() == Some(video.as_str()),
                video,
                info,
            })
        })
        .collect();
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

#[tauri::command]
fn list_clips() -> CmdResult<Vec<ClipSummary>> {
    let dir = config::load().map_err(err)?.recordings_dir().map_err(err)?.join("clips");
    let mut out: Vec<ClipSummary> = dir_files(&dir)
        .into_iter()
        .filter(|p| clip::is_video(p))
        .filter_map(|p| {
            let stem = p.file_stem()?.to_str()?.to_string();
            // Names come from clip.rs: "<recording> <MM>m<SS>s <label>".
            let mut parts = stem.rsplitn(3, ' ');
            let label = parts.next().unwrap_or_default().to_string();
            let at = parts.next().unwrap_or_default().to_string();
            let recording = parts.next().unwrap_or(&stem).to_string();
            let meta = fs::metadata(&p).ok()?;
            Some(ClipSummary {
                kills: label.strip_suffix('殺').and_then(|k| k.parse().ok()),
                name: p.file_name()?.to_string_lossy().into_owned(),
                path: p.to_string_lossy().into_owned(),
                size: meta.len(),
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0),
                recording,
                at,
                label,
            })
        })
        .collect();
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

fn thumb_path(video: &Path) -> PathBuf {
    let name = video.file_name().unwrap_or_default().to_string_lossy();
    video.parent().unwrap_or(Path::new(".")).join(".thumbs").join(format!("{name}.jpg"))
}

/// A cached JPEG of `path` at `at` seconds (negative: from the end).
#[tauri::command]
async fn thumbnail(path: String, at: f64) -> CmdResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let video = PathBuf::from(&path);
        let thumb = thumb_path(&video);
        if !thumb.exists() {
            fs::create_dir_all(thumb.parent().unwrap()).map_err(err)?;
            clip::grab_frame(&video, at, 640, &thumb).map_err(err)?;
        }
        Ok(thumb.to_string_lossy().into_owned())
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn delete_match(video: String) -> CmdResult<()> {
    let video = PathBuf::from(video);
    for p in [MatchInfo::path_for(&video), thumb_path(&video)] {
        if p.exists() {
            trash::delete(&p).map_err(err)?;
        }
    }
    trash::delete(&video).map_err(err)
}

#[tauri::command]
fn delete_clip(path: String) -> CmdResult<()> {
    let path = PathBuf::from(path);
    let thumb = thumb_path(&path);
    if thumb.exists() {
        let _ = fs::remove_file(thumb);
    }
    trash::delete(&path).map_err(err)
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[tauri::command]
fn reveal(path: String) -> CmdResult<()> {
    Command::new("explorer")
        .raw_arg(format!("/select,\"{}\"", path.replace('/', "\\")))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(err)
}

#[tauri::command]
fn open_path(path: String) -> CmdResult<()> {
    Command::new("explorer").arg(path.replace('/', "\\")).spawn().map(|_| ()).map_err(err)
}

#[tauri::command]
async fn cut_range(video: String, start: f64, end: f64) -> CmdResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        clip::cut_range(Path::new(&video), start, end, "精選")
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn clip_match(video: String) -> CmdResult<usize> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = config::load().map_err(err)?;
        clip::clip_match(Path::new(&video), &cfg).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(err)?
}

// ---------- killfeed calibration ----------

#[derive(Serialize)]
struct GameFrame {
    /// JPEG data URL of the current Valorant window.
    image: String,
    width: u32,
    height: u32,
}

/// Grabs one frame of the Valorant window for the calibration screen.
#[tauri::command]
async fn grab_game_frame() -> CmdResult<GameFrame> {
    tauri::async_runtime::spawn_blocking(|| {
        let window = ownclips::capture::find_valorant_window().map_err(err)?;
        let (control, sink) = ownclips::capture::start_capture(window).map_err(err)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let frame = loop {
            if let Some(f) = sink.lock().unwrap().latest.clone().filter(|f| f.width > 0) {
                break Some(f);
            }
            if Instant::now() > deadline {
                break None;
            }
            thread::sleep(Duration::from_millis(30));
        };
        let _ = control.stop();
        let frame = frame.ok_or("沒有擷取到遊戲畫面,請確認 Valorant 視窗沒有被最小化")?;
        let img = frame.to_image().ok_or("擷取畫面資料大小不一致")?;
        let rgb = image::DynamicImage::ImageRgba8(img).into_rgb8();
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut Cursor::new(&mut jpeg), 88)
            .encode_image(&rgb)
            .map_err(err)?;
        Ok(GameFrame {
            image: format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(jpeg)),
            width: frame.width,
            height: frame.height,
        })
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn save_region(region: RegionPct) -> CmdResult<()> {
    let mut cfg = config::load().map_err(err)?;
    cfg.killfeed_region = Some(region);
    config::save(&cfg).map(|_| ()).map_err(err)
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let bg = start_background(app.handle().clone());
            app.manage(bg);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_config,
            save_config,
            recordings_dir,
            ffmpeg_available,
            list_matches,
            list_clips,
            thumbnail,
            delete_match,
            delete_clip,
            reveal,
            open_path,
            cut_range,
            clip_match,
            grab_game_frame,
            save_region,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build app");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            // Finish a recording in progress so the file stays playable.
            let bg = app.state::<Background>();
            bg.stop.store(true, Ordering::Relaxed);
            let thread = bg.thread.lock().unwrap().take();
            if let Some(t) = thread {
                let _ = t.join();
            }
        }
    });
}
