use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use serde::Serialize;
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use crate::config::RegionPct;
use crate::gamestate::Game;

/// One captured RGBA frame (or a region of it), copied out of the capture
/// thread so the rest of the app never has to touch the underlying D3D
/// buffer.
#[derive(Default, Clone)]
pub struct FrameData {
    pub width: u32,
    pub height: u32,
    /// Height of the whole window frame, also when only a region was copied.
    pub source_height: u32,
    pub rgba: Vec<u8>,
}

impl FrameData {
    pub fn to_image(&self) -> Option<image::RgbaImage> {
        image::RgbaImage::from_raw(self.width, self.height, self.rgba.clone())
    }

    pub fn to_rgb(&self) -> Option<image::RgbImage> {
        Some(image::DynamicImage::ImageRgba8(self.to_image()?).into_rgb8())
    }
}

/// `region` in pixels of a `width` x `height` frame, as (x0, y0, x1, y1).
pub fn region_px(region: RegionPct, width: u32, height: u32) -> (u32, u32, u32, u32) {
    let px = |f: f32, size: u32| ((f * size as f32).round().max(0.0) as u32).min(size);
    let (x0, y0) = (px(region.x, width).min(width - 1), px(region.y, height).min(height - 1));
    let (x1, y1) = (px(region.x + region.w, width).max(x0 + 1), px(region.y + region.h, height).max(y0 + 1));
    (x0, y0, x1, y1)
}

#[derive(Default)]
pub struct FrameSink {
    pub latest: Option<FrameData>,
    /// Bumped on every new frame so consumers can cheaply detect a change
    /// without comparing the whole buffer.
    pub generation: u64,
}

pub type SharedSink = Arc<Mutex<FrameSink>>;

pub struct CaptureHandler {
    sink: SharedSink,
    region: Option<RegionPct>,
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = (SharedSink, Option<RegionPct>);
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (sink, region) = ctx.flags;
        Ok(Self { sink, region })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame<'_>,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let source_height = frame.height();
        // Copying only the region keeps the GPU readback (which stalls until
        // the GPU catches up) small: the killfeed is ~1/16 of the frame.
        let buffer = match self.region {
            Some(r) if frame.width() > 1 && source_height > 1 => {
                let (x0, y0, x1, y1) = region_px(r, frame.width(), source_height);
                frame.buffer_crop(x0, y0, x1, y1)?
            }
            _ => frame.buffer()?,
        };
        let width = buffer.width();
        let height = buffer.height();

        let mut scratch = Vec::new();
        let rgba = buffer.as_nopadding_buffer(&mut scratch).to_vec();

        let mut sink = self.sink.lock().unwrap();
        sink.latest = Some(FrameData {
            width,
            height,
            source_height,
            rgba,
        });
        sink.generation = sink.generation.wrapping_add(1);

        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub type Control = CaptureControl<CaptureHandler, Box<dyn std::error::Error + Send + Sync>>;

/// Finds the Valorant window. The game must already be running (menu or
/// in-match both work) since there is nothing to capture otherwise.
pub fn find_valorant_window() -> Result<Window> {
    Window::from_contains_name("VALORANT")
        .map_err(|e| anyhow!("找不到 Valorant 視窗,請先啟動遊戲並進入遊戲畫面再試一次: {e}"))
}

/// Finds the window of a running match of `game`.
pub fn find_game_window(game: Game) -> Result<Window> {
    match game {
        Game::Valorant => find_valorant_window(),
        // The match itself, not the lobby client ("League of Legends").
        Game::League => Window::from_contains_name("League of Legends (TM) Client")
            .map_err(|e| anyhow!("找不到英雄聯盟的遊戲視窗: {e}")),
        Game::Other => Err(anyhow!("手動錄影要自己選視窗")),
    }
}

/// A window that can be recorded by hand.
#[derive(Serialize, Clone, Debug)]
pub struct WindowInfo {
    pub hwnd: usize,
    pub title: String,
    pub process: String,
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmGetWindowAttribute(hwnd: *mut std::ffi::c_void, attribute: u32, value: *mut u32, size: u32) -> i32;
}

const DWMWA_CLOAKED: u32 = 14;

/// Cloaked windows count as visible but aren't shown, like the suspended
/// background windows of Windows' own apps.
fn is_cloaked(window: &Window) -> bool {
    let mut cloaked = 0u32;
    let hr = unsafe { DwmGetWindowAttribute(window.as_raw_hwnd(), DWMWA_CLOAKED, &mut cloaked, 4) };
    hr == 0 && cloaked != 0
}

/// The windows that can be recorded, front to back: not our own, not
/// cloaked, and not minimized ones (they sit at -32000 and deliver no frames).
pub fn list_windows() -> Result<Vec<WindowInfo>> {
    let own_pid = std::process::id();
    let windows = Window::enumerate().map_err(|e| anyhow!("列出視窗失敗: {e}"))?;
    Ok(windows
        .into_iter()
        .filter_map(|w| {
            let title = w.title().ok().filter(|t| !t.trim().is_empty() && t != "Program Manager")?;
            if w.process_id().ok()? == own_pid || w.rect().ok()?.left <= -32000 || is_cloaked(&w) {
                return None;
            }
            Some(WindowInfo { hwnd: w.as_raw_hwnd() as usize, title, process: w.process_name().unwrap_or_default() })
        })
        .collect())
}

/// Starts a background capture of `window` and returns a handle to stop it
/// plus a shared sink that always holds the most recent frame.
pub fn start_capture(window: Window) -> Result<(Control, SharedSink)> {
    start_capture_every(window, None, None)
}

/// Like [`start_capture`], but delivering at most one frame per `interval`
/// (every frame is copied to the CPU, so throttle long-running captures)
/// and, given a `region`, only that part of each frame.
pub fn start_capture_every(
    window: Window,
    interval: Option<std::time::Duration>,
    region: Option<RegionPct>,
) -> Result<(Control, SharedSink)> {
    let sink: SharedSink = Arc::new(Mutex::new(FrameSink::default()));

    let settings = Settings::new(
        window,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        interval.map_or(MinimumUpdateIntervalSettings::Default, MinimumUpdateIntervalSettings::Custom),
        DirtyRegionSettings::Default,
        ColorFormat::Rgba8,
        (sink.clone(), region),
    );

    let control = CaptureHandler::start_free_threaded(settings)
        .map_err(|e| anyhow!("啟動視窗擷取失敗: {e}"))?;

    Ok((control, sink))
}
