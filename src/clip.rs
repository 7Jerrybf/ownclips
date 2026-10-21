// Cuts highlight clips out of full-match recordings using the kill events
// the auto recorder writes next to each recording (`<recording>.events.json`). Kills
// close enough together that their clip windows overlap are merged into a
// single multi-kill clip. Clips go into a `clips` folder beside the
// recordings; ones that already exist are skipped, so it is safe to rerun.
//
// Uses ffmpeg with stream copy (no re-encode): fast and lossless, at the
// cost of the clip starting on the keyframe at or just before the window.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use anyhow::{anyhow, Context, Result};

use crate::config::{self, Config};
use crate::session::{EventKind, MatchInfo};

const VIDEO_EXTS: [&str; 4] = ["mp4", "mkv", "mov", "flv"];

pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

struct Segment {
    start: f64,
    end: f64,
    kills: usize,
}

pub fn run() -> Result<()> {
    let cfg = config::load()?;
    let dir = cfg.recordings_dir()?;
    let mut videos: Vec<PathBuf> = fs::read_dir(&dir)
        .with_context(|| format!("讀取錄影資料夾失敗: {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_video(p) && MatchInfo::path_for(p).exists())
        .collect();
    videos.sort();
    if videos.is_empty() {
        println!("{} 裡沒有找到帶事件紀錄的錄影。", dir.display());
        return Ok(());
    }
    println!("片段會存到: {}", dir.join("clips").display());

    let mut made = 0;
    for video in &videos {
        match clip_match(video, &cfg) {
            Ok(n) => made += n,
            Err(e) => eprintln!("  失敗: {e:#}"),
        }
    }
    println!();
    println!("完成,這次新增 {made} 個片段。");
    Ok(())
}

/// Cuts a clip around each kill of the recording `video` into the `clips`
/// folder beside it; returns how many new clips were made.
pub fn clip_match(video: &Path, cfg: &Config) -> Result<usize> {
    let info = MatchInfo::load(video).ok_or_else(|| anyhow!("讀不到事件紀錄: {}", video.display()))?;
    let mut kills: Vec<f64> = info.events.iter().filter(|e| e.kind == EventKind::Kill).map(|e| e.t).collect();
    kills.sort_by(|a, b| a.total_cmp(b));
    let out_dir = video.parent().unwrap_or(Path::new(".")).join("clips");
    fs::create_dir_all(&out_dir).with_context(|| format!("建立片段資料夾失敗: {}", out_dir.display()))?;
    let segs = segments(&kills, cfg.clip_pre_seconds, cfg.clip_post_seconds);
    cut_segments(video, &segs, &out_dir)
}

fn segments(kills: &[f64], pre: f64, post: f64) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    for &t in kills {
        let (start, end) = ((t - pre).max(0.0), t + post);
        match out.last_mut() {
            Some(last) if start <= last.end => {
                last.end = end;
                last.kills += 1;
            }
            _ => out.push(Segment { start, end, kills: 1 }),
        }
    }
    out
}

fn cut_segments(video: &Path, segs: &[Segment], out_dir: &Path) -> Result<usize> {
    let stem = video.file_stem().and_then(|s| s.to_str()).unwrap_or("recording");
    let ext = video.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    println!();
    println!("{} ({} 個片段)", video.display(), segs.len());

    let mut made = 0;
    for seg in segs {
        let at = seg.start as u64;
        let name = format!("{stem} {:02}m{:02}s {}殺.{ext}", at / 60, at % 60, seg.kills);
        let out = out_dir.join(&name);
        if out.exists() {
            println!("  已存在,略過: {name}");
            continue;
        }
        ffmpeg_cut(video, seg.start, seg.end - seg.start, &out)?;
        println!("  {name}");
        made += 1;
    }
    Ok(made)
}

/// Cuts `start..end` (seconds) of `video` into its `clips` folder, named
/// with `label` (e.g. "精選"); returns the new clip's path.
pub fn cut_range(video: &Path, start: f64, end: f64, label: &str) -> Result<PathBuf> {
    if end <= start {
        return Err(anyhow!("終點必須在起點之後"));
    }
    let stem = video.file_stem().and_then(|s| s.to_str()).unwrap_or("recording");
    let ext = video.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let out_dir = video.parent().unwrap_or(Path::new(".")).join("clips");
    fs::create_dir_all(&out_dir)?;
    let at = start as u64;
    let mut out = out_dir.join(format!("{stem} {:02}m{:02}s {label}.{ext}", at / 60, at % 60));
    let mut n = 2;
    while out.exists() {
        out = out_dir.join(format!("{stem} {:02}m{:02}s {label}{n}.{ext}", at / 60, at % 60));
        n += 1;
    }
    ffmpeg_cut(video, start, end - start, &out)?;
    Ok(out)
}

/// Saves one frame of `video` as a JPEG `width` pixels wide. A negative
/// `at` counts back from the end.
pub fn grab_frame(video: &Path, at: f64, width: u32, out: &Path) -> Result<()> {
    let seek = if at < 0.0 { "-sseof" } else { "-ss" };
    let status = ffmpeg()?
        .args(["-hide_banner", "-loglevel", "error", "-y", seek, &format!("{at:.2}"), "-i"])
        .arg(video)
        .args(["-frames:v", "1", "-vf", &format!("scale={width}:-2"), "-q:v", "4"])
        .arg(out)
        .status()?;
    if !status.success() {
        return Err(anyhow!("擷取畫面失敗: {}", video.display()));
    }
    Ok(())
}

pub fn ffmpeg_available() -> bool {
    ffmpeg_exe().is_some()
}

fn ffmpeg_cut(video: &Path, start: f64, duration: f64, out: &Path) -> Result<()> {
    let status = ffmpeg()?
        .args(["-hide_banner", "-loglevel", "error", "-n"])
        .args(["-ss", &format!("{start:.2}")])
        .arg("-i")
        .arg(video)
        .args(["-t", &format!("{duration:.2}")])
        .args(["-c", "copy", "-avoid_negative_ts", "make_zero"])
        .arg(out)
        .status()?;
    if !status.success() {
        let _ = fs::remove_file(out);
        return Err(anyhow!("ffmpeg 裁切失敗: {}。如果這支錄影還在錄,請等對局結束再裁切。", out.display()));
    }
    Ok(())
}

/// An ffmpeg command that won't flash a console window from the GUI app.
fn ffmpeg() -> Result<Command> {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let exe = ffmpeg_exe().ok_or_else(|| anyhow!("找不到 ffmpeg,請先安裝: winget install Gyan.FFmpeg"))?;
    let mut cmd = Command::new(exe);
    cmd.creation_flags(CREATE_NO_WINDOW);
    Ok(cmd)
}

/// Finds ffmpeg on PATH, or where winget installs it (PATH changes only
/// reach programs started after the install).
fn ffmpeg_exe() -> Option<PathBuf> {
    static EXE: OnceLock<Option<PathBuf>> = OnceLock::new();
    EXE.get_or_init(|| {
        let on_path = std::env::var_os("PATH")
            .into_iter()
            .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
            .map(|d| d.join("ffmpeg.exe"));
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        let winget_link = local.iter().map(|l| l.join(r"Microsoft\WinGet\Links\ffmpeg.exe"));
        let winget_pkgs = local
            .iter()
            .filter_map(|l| fs::read_dir(l.join(r"Microsoft\WinGet\Packages")).ok())
            .flatten()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("Gyan.FFmpeg")))
            .filter_map(|p| fs::read_dir(p).ok())
            .flatten()
            .filter_map(|e| e.ok().map(|e| e.path().join(r"bin\ffmpeg.exe")));
        on_path.chain(winget_link).chain(winget_pkgs).find(|p| p.is_file())
    })
    .clone()
}
