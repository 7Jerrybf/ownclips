use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::Serialize;
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings, GraphicsCaptureItemType,
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
        Game::Other => Err(anyhow!("沒有固定的遊戲視窗")),
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

#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[link(name = "user32")]
extern "system" {
    fn GetClientRect(hwnd: *mut std::ffi::c_void, rect: *mut Rect) -> i32;
}

/// Size of the window's client area (what gets recorded), in pixels.
pub fn client_size(hwnd: usize) -> Option<(u32, u32)> {
    let mut r = Rect::default();
    let ok = unsafe { GetClientRect(hwnd as *mut std::ffi::c_void, &mut r) } != 0;
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    (ok && w > 0 && h > 0).then_some((w as u32, h as u32))
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
    let windows = Window::enumerate().map_err(|e| anyhow!("列出視窗失敗: {e}"))?;
    Ok(windows.iter().filter_map(recordable).collect())
}

fn recordable(w: &Window) -> Option<WindowInfo> {
    let title = w.title().ok().filter(|t| !t.trim().is_empty() && t != "Program Manager")?;
    if w.process_id().ok()? == std::process::id() || w.rect().ok()?.left <= -32000 || is_cloaked(w) {
        return None;
    }
    let hwnd = w.as_raw_hwnd() as usize;
    // Games guarded by an anti-cheat only give their name to the lighter query.
    let process = crate::gamedetect::exe_name(hwnd).or_else(|| w.process_name().ok()).unwrap_or_default();
    Some(WindowInfo { hwnd, title, process })
}

/// A screen that can be recorded.
#[derive(Serialize, Clone, Debug)]
pub struct MonitorInfo {
    pub hmonitor: usize,
    /// 1-based, as Windows numbers the displays.
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
}

/// The connected screens.
pub fn list_monitors() -> Result<Vec<MonitorInfo>> {
    let monitors = Monitor::enumerate().map_err(|e| anyhow!("列出螢幕失敗: {e}"))?;
    Ok(monitors.iter().enumerate().map(|(i, m)| monitor_info(m, i + 1)).collect())
}

fn monitor_info(m: &Monitor, fallback_index: usize) -> MonitorInfo {
    MonitorInfo {
        hmonitor: m.as_raw_hmonitor() as usize,
        index: m.index().unwrap_or(fallback_index),
        name: m.name().unwrap_or_default(),
        width: m.width().unwrap_or(0),
        height: m.height().unwrap_or(0),
    }
}

/// The screen with the handle `hmonitor`.
pub fn monitor(hmonitor: usize) -> MonitorInfo {
    monitor_info(&Monitor::from_raw_hmonitor(hmonitor as *mut std::ffi::c_void), 1)
}

/// The window in front, if it can be recorded.
pub fn foreground_window() -> Option<WindowInfo> {
    Window::foreground().ok().filter(Window::is_valid).as_ref().and_then(recordable)
}

/// One frame of `item` (a window or a screen), or an error if none arrives
/// within `timeout`.
pub fn grab_frame<T>(item: T, timeout: Duration) -> Result<FrameData>
where
    T: TryInto<GraphicsCaptureItemType> + Send + 'static,
{
    let (control, sink) = start_capture_every(item, None, None)?;
    let deadline = Instant::now() + timeout;
    let frame = loop {
        if let Some(f) = sink.lock().unwrap().latest.clone().filter(|f| f.width > 0) {
            break Some(f);
        }
        if Instant::now() > deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    let _ = control.stop();
    frame.ok_or_else(|| anyhow!("沒有擷取到視窗畫面,請確認視窗沒有被最小化"))
}

/// A small image of the window `hwnd`, at most `width` pixels wide.
pub fn window_thumbnail(hwnd: usize, width: u32) -> Result<image::RgbImage> {
    let window = Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void);
    thumbnail(grab_frame(window, Duration::from_secs(2))?, width)
}

/// A small image of the screen `hmonitor`, at most `width` pixels wide.
pub fn monitor_thumbnail(hmonitor: usize, width: u32) -> Result<image::RgbImage> {
    let monitor = Monitor::from_raw_hmonitor(hmonitor as *mut std::ffi::c_void);
    thumbnail(grab_frame(monitor, Duration::from_secs(2))?, width)
}

fn thumbnail(frame: FrameData, width: u32) -> Result<image::RgbImage> {
    let img = frame.to_rgb().ok_or_else(|| anyhow!("擷取畫面資料大小不一致"))?;
    let width = width.min(img.width()).max(1);
    let height = (img.height() as u64 * width as u64 / img.width().max(1) as u64).max(1) as u32;
    Ok(image::imageops::thumbnail(&img, width, height))
}

/// Starts a background capture of `item` (a window or a screen) and returns
/// a handle to stop it plus a shared sink that always holds the most recent
/// frame. Delivers at most one frame per `interval` (every frame is copied
/// to the CPU, so throttle long-running captures) and, given a `region`,
/// only that part of each frame.
pub fn start_capture_every<T>(
    item: T,
    interval: Option<Duration>,
    region: Option<RegionPct>,
) -> Result<(Control, SharedSink)>
where
    T: TryInto<GraphicsCaptureItemType> + Send + 'static,
{
    let sink: SharedSink = Arc::new(Mutex::new(FrameSink::default()));

    let settings = Settings::new(
        item,
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
