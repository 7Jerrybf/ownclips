use std::fs;
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

use crate::audio::AudioMode;
use crate::recorder::{RecordSettings, VideoCodec, VideoQuality};

/// A screen region expressed as fractions (0.0..1.0) of the captured
/// window's client width/height, so it stays correct no matter what
/// resolution, aspect ratio, or window mode the game is running in.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct RegionPct {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Missing fields (e.g. from an older config file) take their defaults.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Config {
    pub killfeed_region: Option<RegionPct>,
    /// Where recordings go; None means Videos\OwnClips.
    pub recordings_dir: Option<String>,
    /// Highlight ranges: seconds before the first and after the last kill.
    pub clip_pre_seconds: f64,
    pub clip_post_seconds: f64,
    /// Built-in recorder settings.
    pub audio_mode: AudioMode,
    pub video_fps: u32,
    pub video_codec: VideoCodec,
    pub video_quality: VideoQuality,
    /// H.264 only; AV1 uses `video_quality`.
    pub video_bitrate_mbps: u32,
    /// Closing the window hides it to the tray and keeps recording.
    pub close_to_tray: bool,
}

/// Where the killfeed sits with Valorant's default HUD on a 16:9 screen
/// (measured at 2560x1440), so detection works before any calibration.
pub const DEFAULT_KILLFEED_REGION: RegionPct = RegionPct { x: 0.761, y: 0.0728, w: 0.2282, h: 0.2669 };

impl Default for Config {
    fn default() -> Self {
        Self {
            killfeed_region: Some(DEFAULT_KILLFEED_REGION),
            recordings_dir: None,
            clip_pre_seconds: 8.0,
            clip_post_seconds: 3.0,
            audio_mode: AudioMode::Game,
            video_fps: 60,
            video_codec: VideoCodec::Av1,
            video_quality: VideoQuality::Standard,
            video_bitrate_mbps: 20,
            close_to_tray: true,
        }
    }
}

impl Config {
    /// Where recordings go: the configured folder, or Videos\OwnClips.
    pub fn recordings_dir(&self) -> Result<PathBuf> {
        match &self.recordings_dir {
            Some(d) => Ok(PathBuf::from(d.replace('/', "\\"))),
            None => {
                let home = std::env::var("USERPROFILE").context("找不到 USERPROFILE 環境變數")?;
                Ok(PathBuf::from(home).join("Videos").join("OwnClips"))
            }
        }
    }

    pub fn record_settings(&self) -> RecordSettings {
        RecordSettings {
            fps: self.video_fps,
            bitrate_mbps: self.video_bitrate_mbps,
            codec: self.video_codec,
            quality: self.video_quality,
            audio: self.audio_mode,
            detect_region: self.killfeed_region.map(detect_region),
        }
    }
}

/// Killfeed rows are right-aligned and grow to the left with long names and
/// assist icons, so the detector watches at least this much of the window's
/// width left of the calibrated region's right edge.
pub const MIN_DETECT_WIDTH: f32 = 0.40;

/// The region the detector actually watches for a calibrated `region`.
pub fn detect_region(region: RegionPct) -> RegionPct {
    let right = (region.x + region.w).min(1.0);
    let x = region.x.min(right - MIN_DETECT_WIDTH).max(0.0);
    RegionPct { x, w: right - x, ..region }
}

fn config_dir() -> Result<PathBuf> {
    let appdata = std::env::var("APPDATA").context("找不到 APPDATA 環境變數")?;
    Ok(PathBuf::from(appdata).join("OwnClips"))
}

fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

pub fn load() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("讀取設定檔失敗: {}", path.display()))?;
    let config = serde_json::from_str(&text).with_context(|| format!("設定檔格式錯誤: {}", path.display()))?;
    Ok(config)
}

pub fn save(config: &Config) -> Result<PathBuf> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir).with_context(|| format!("建立設定資料夾失敗: {}", dir.display()))?;
    let path = config_path()?;
    let text = serde_json::to_string_pretty(config)?;
    fs::write(&path, text).with_context(|| format!("寫入設定檔失敗: {}", path.display()))?;
    Ok(path)
}

/// Directory the detector saves the killfeed crop into on every detected
/// kill, so each marker can be checked against what the detector saw.
pub fn detect_samples_dir() -> Result<PathBuf> {
    let dir = config_dir()?.join("detect_samples");
    fs::create_dir_all(&dir).with_context(|| format!("建立樣本資料夾失敗: {}", dir.display()))?;
    Ok(dir)
}

/// Directory the auto recorder saves killfeed frames with unexplained
/// yellow into, to learn what assists look like.
pub fn assist_samples_dir() -> Result<PathBuf> {
    let dir = config_dir()?.join("assist_samples");
    fs::create_dir_all(&dir).with_context(|| format!("建立樣本資料夾失敗: {}", dir.display()))?;
    Ok(dir)
}
