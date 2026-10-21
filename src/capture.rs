use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use crate::config::RegionPct;

/// One captured RGBA frame, copied out of the capture thread so the rest of
/// the app never has to touch the underlying D3D buffer.
#[derive(Default, Clone)]
pub struct FrameData {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl FrameData {
    /// Crops `region` out of the frame at native resolution.
    pub fn crop_rgb(&self, region: RegionPct) -> Result<image::RgbImage> {
        let rgba = image::RgbaImage::from_raw(self.width, self.height, self.rgba.clone())
            .ok_or_else(|| anyhow!("擷取畫面資料大小不一致"))?;
        let x = ((region.x * self.width as f32).round() as u32).min(self.width.saturating_sub(1));
        let y = ((region.y * self.height as f32).round() as u32).min(self.height.saturating_sub(1));
        let w = ((region.w * self.width as f32).round() as u32).max(1).min(self.width - x);
        let h = ((region.h * self.height as f32).round() as u32).max(1).min(self.height - y);
        let cropped = image::imageops::crop_imm(&rgba, x, y, w, h).to_image();
        Ok(image::DynamicImage::ImageRgba8(cropped).into_rgb8())
    }

    pub fn to_image(&self) -> Option<image::RgbaImage> {
        image::RgbaImage::from_raw(self.width, self.height, self.rgba.clone())
    }
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
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = SharedSink;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self { sink: ctx.flags })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame<'_>,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let buffer = frame.buffer()?;
        let width = buffer.width();
        let height = buffer.height();

        let mut scratch = Vec::new();
        let rgba = buffer.as_nopadding_buffer(&mut scratch).to_vec();

        let mut sink = self.sink.lock().unwrap();
        sink.latest = Some(FrameData {
            width,
            height,
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

/// Starts a background capture of `window` and returns a handle to stop it
/// plus a shared sink that always holds the most recent frame.
pub fn start_capture(window: Window) -> Result<(Control, SharedSink)> {
    start_capture_every(window, None)
}

/// Like [`start_capture`], but delivering at most one frame per `interval`
/// (every frame is copied to the CPU, so throttle long-running captures).
pub fn start_capture_every(window: Window, interval: Option<std::time::Duration>) -> Result<(Control, SharedSink)> {
    let sink: SharedSink = Arc::new(Mutex::new(FrameSink::default()));

    let settings = Settings::new(
        window,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        interval.map_or(MinimumUpdateIntervalSettings::Default, MinimumUpdateIntervalSettings::Custom),
        DirtyRegionSettings::Default,
        ColorFormat::Rgba8,
        sink.clone(),
    );

    let control = CaptureHandler::start_free_threaded(settings)
        .map_err(|e| anyhow!("啟動視窗擷取失敗: {e}"))?;

    Ok((control, sink))
}
