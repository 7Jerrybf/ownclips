// OwnClips desktop app backend: runs the auto recorder in a background
// thread (reporting to the UI and the tray icon through events) and exposes
// the recordings, clips, settings and killfeed calibration to the web
// frontend. Closing the window keeps it running in the tray.
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
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent, Wry};
use tauri_plugin_autostart::ManagerExt as _;

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
    assists: u32,
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
                    EventKind::Assist => s.assists += 1,
                }
                None
            }
            Notice::Processing { .. } => {
                *s = RecorderStatus { state: "waiting".into(), ..Default::default() };
                let _ = app.emit("library-changed", ());
                Some(Toast { kind: "info".into(), message: "對局結束,正在合成錄影檔…".into() })
            }
            Notice::Recovered { video } => {
                let _ = app.emit("library-changed", ());
                let name = video.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                Some(Toast { kind: "success".into(), message: format!("已救回上次未完成的錄影 {name}") })
            }
            Notice::Stopped { .. } => {
                *s = RecorderStatus { state: "waiting".into(), ..Default::default() };
                let _ = app.emit("library-changed", ());
                Some(Toast { kind: "stop".into(), message: "錄影已存檔,精華區間已標好".into() })
            }
            Notice::Skipped { reason } => Some(Toast { kind: "info".into(), message: reason }),
            Notice::Error(e) => {
                s.last_error = Some(e.clone());
                Some(Toast { kind: "error".into(), message: e })
            }
        }
    };
    let current = status.lock().unwrap().clone();
    update_tray(app, &current);
    let _ = app.emit("recorder-status", current);
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

// ---------- tray ----------

/// Passed by the Windows autostart entry: start hidden in the tray.
const BACKGROUND_ARG: &str = "--background";

struct Tray {
    icon: TrayIcon,
    status_item: MenuItem<Wry>,
    idle: Image<'static>,
    recording: Image<'static>,
}

/// The app icon, and the same icon with a red dot for "recording".
fn tray_images() -> (Image<'static>, Image<'static>) {
    let idle = image::load_from_memory(include_bytes!("../icons/32x32.png")).expect("bundled icon").into_rgba8();
    let mut rec = idle.clone();
    let (w, h) = rec.dimensions();
    let r = w as f32 * 0.22;
    let (cx, cy) = (w as f32 - r - 1.0, h as f32 - r - 1.0);
    for (x, y, p) in rec.enumerate_pixels_mut() {
        let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
        if d <= r {
            *p = image::Rgba([239, 68, 68, 255]);
        } else if d <= r + 1.5 {
            *p = image::Rgba([255, 255, 255, 255]);
        }
    }
    let to_image = |img: image::RgbaImage| Image::new_owned(img.into_raw(), w, h);
    (to_image(idle), to_image(rec))
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<Tray> {
    let (idle, recording) = tray_images();
    let status_item = MenuItem::with_id(app, "status", "等待對局中", false, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &status_item,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "open", "開啟 OwnClips", true, None::<&str>)?,
            &MenuItem::with_id(app, "folder", "開啟錄影資料夾", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "結束 OwnClips", true, None::<&str>)?,
        ],
    )?;
    let icon = TrayIconBuilder::with_id("main")
        .icon(idle.clone())
        .tooltip("OwnClips · 等待對局中")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "folder" => {
                if let Ok(dir) = config::load().and_then(|c| c.recordings_dir()) {
                    let _ = fs::create_dir_all(&dir);
                    let _ = open_path(dir.to_string_lossy().into_owned());
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(Tray { icon, status_item, idle, recording })
}

fn update_tray(app: &AppHandle, s: &RecorderStatus) {
    let Some(tray) = app.try_state::<Tray>() else { return };
    let recording = s.state == "recording";
    let text = if recording {
        format!(
            "錄影中 · {} · {}  {}/{}/{}",
            s.mode.as_deref().unwrap_or("?"),
            s.map.as_deref().unwrap_or("?"),
            s.kills,
            s.deaths,
            s.assists
        )
    } else {
        "等待對局中".to_string()
    };
    let _ = tray.status_item.set_text(&text);
    let _ = tray.icon.set_tooltip(Some(format!("OwnClips · {text}")));
    let _ = tray.icon.set_icon(Some(if recording { tray.recording.clone() } else { tray.idle.clone() }));
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> CmdResult<bool> {
    app.autolaunch().is_enabled().map_err(err)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<()> {
    let launcher = app.autolaunch();
    if enabled { launcher.enable() } else { launcher.disable() }.map_err(err)
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
        // An .mkv is a recording still in progress or being muxed; once the
        // .mp4 exists that one is the match.
        .filter(|p| !(is_mkv(p) && p.with_extension("mp4").exists()))
        .filter_map(|p| {
            let info = MatchInfo::load(&p)?;
            let video = p.to_string_lossy().into_owned();
            Some(MatchSummary {
                name: p.file_stem()?.to_string_lossy().into_owned(),
                size: fs::metadata(&p).map(|m| m.len()).unwrap_or(0),
                recording: is_mkv(&p) || current.as_deref() == Some(video.as_str()),
                video,
                info,
            })
        })
        .collect();
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

fn is_mkv(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mkv"))
}

/// The first call probes the encoders with ffmpeg, so keep it off the UI thread.
#[tauri::command]
async fn encoder_name(codec: ownclips::recorder::VideoCodec) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || ownclips::recorder::encoder_name(codec).map(String::from))
        .await
        .ok()
        .flatten()
}

#[tauri::command]
fn delete_match(video: String) -> CmdResult<()> {
    let video = PathBuf::from(video);
    for p in [MatchInfo::path_for(&video)] {
        if p.exists() {
            trash::delete(&p).map_err(err)?;
        }
    }
    trash::delete(&video).map_err(err)
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

/// Runs the current detector over a finished recording again, reporting
/// progress (0..1) as "reanalyze-progress" events.
#[tauri::command]
async fn reanalyze_match(app: AppHandle, video: String) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(&video);
        ownclips::session::reanalyze(&path, |p| {
            let _ = app.emit("reanalyze-progress", (&video, p));
        })
        .map_err(|e| format!("{e:#}"))?;
        let _ = app.emit("library-changed", ());
        Ok(())
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
        // Two copies would both record every match: a second launch just
        // brings the running window forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![BACKGROUND_ARG]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if config::load().map(|c| c.close_to_tray).unwrap_or(true) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            // The installer ships ffmpeg next to the app.
            if let Ok(dir) = app.path().resource_dir() {
                ownclips::clip::set_bundled_ffmpeg(dir.join("ffmpeg").join("ffmpeg.exe"));
            }
            let tray = build_tray(app.handle())?;
            app.manage(tray);
            let bg = start_background(app.handle().clone());
            app.manage(bg);
            // The window starts hidden; launched by autostart it stays in the tray.
            if !std::env::args().any(|a| a == BACKGROUND_ARG) {
                show_main(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_config,
            save_config,
            recordings_dir,
            ffmpeg_available,
            encoder_name,
            list_matches,
            delete_match,
            reveal,
            open_path,
            cut_range,
            reanalyze_match,
            grab_game_frame,
            save_region,
            get_autostart,
            set_autostart,
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
