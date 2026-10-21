// Command-line front end of OwnClips (the desktop app is in app/).
use std::env;

use anyhow::Result;
use ownclips::{clip, config, recorder, session};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("detect") => session::run(),
        Some("clip") => clip::run(),
        Some("record") => record_test(args.get(2), args.get(3), args.get(4)),
        _ => {
            println!("Usage: ownclips <detect|clip|record>");
            println!("  detect                         - auto-record each match and log your kills/deaths");
            println!("  clip                           - cut a highlight clip around each kill from the recordings");
            println!("  record [secs] [window] [audio] - test the built-in recorder (default 10s, VALORANT, audio mode from settings)");
            Ok(())
        }
    }
}

/// Records `secs` seconds of the first window whose title contains `name`,
/// to try out the built-in recorder without a match.
fn record_test(secs: Option<&String>, name: Option<&String>, audio: Option<&String>) -> Result<()> {
    let secs: u64 = secs.and_then(|s| s.parse().ok()).unwrap_or(10);
    let name = name.map(String::as_str).unwrap_or("VALORANT");
    let cfg = config::load()?;
    let window = windows_capture::window::Window::from_contains_name(name)
        .map_err(|e| anyhow::anyhow!("找不到標題包含 \"{name}\" 的視窗: {e}"))?;
    let dir = cfg.recordings_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.mp4", chrono::Local::now().format("%Y-%m-%d %H-%M-%S")));
    println!("錄影 \"{}\" {secs} 秒 -> {}", window.title().unwrap_or_default(), path.display());
    let mut settings = cfg.record_settings();
    if let Some(a) = audio {
        settings.audio = serde_json::from_value(serde_json::Value::String(a.clone()))
            .map_err(|_| anyhow::anyhow!("聲音模式只能是 game / system / game_and_mic / off"))?;
    }
    let mut rec = recorder::Recorder::start(window, &path, settings)?;
    std::thread::sleep(std::time::Duration::from_secs(secs));
    if let Some(e) = rec.error() {
        println!("錄影中發生錯誤: {e}");
    }
    println!("合成錄影檔...");
    let path = rec.stop()?.finish()?;
    println!("完成: {}", path.display());
    Ok(())
}
