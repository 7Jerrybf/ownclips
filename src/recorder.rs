// Built-in recorder (replaces OBS): captures the Valorant window with
// Windows.Graphics.Capture and feeds the frames, still on the GPU, into a
// Media Foundation H.264 encoder (hardware accelerated), with audio from
// audio.rs. H.264/AAC MP4 is what the app's built-in player can play.
//
// The same capture also feeds the detector: every DETECT_FRAME_INTERVAL a
// frame is copied to the CPU into the shared FrameSink, instead of copying
// every frame as the detection-only capture does.
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder, VideoSettingsSubType,
};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings, MinimumUpdateIntervalSettings,
    SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use crate::audio::{self, AudioCapture, AudioMode};
use crate::capture::{FrameData, FrameSink, SharedSink};

const DETECT_FRAME_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy)]
pub struct RecordSettings {
    pub fps: u32,
    pub bitrate_mbps: u32,
    pub audio: AudioMode,
}

struct Shared {
    path: PathBuf,
    settings: RecordSettings,
    /// Created on the first frame, once the real frame size is known.
    encoder: Mutex<Option<VideoEncoder>>,
    /// When the first frame was encoded: time zero of the recording.
    started: Arc<Mutex<Option<Instant>>>,
    sink: SharedSink,
    last_detect_copy: Mutex<Option<Instant>>,
    error: Mutex<Option<String>>,
}

struct Handler {
    shared: Arc<Shared>,
}

type Error = Box<dyn std::error::Error + Send + Sync>;

impl GraphicsCaptureApiHandler for Handler {
    type Flags = Arc<Shared>;
    type Error = Error;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self { shared: ctx.flags })
    }

    fn on_frame_arrived(&mut self, frame: &mut Frame<'_>, _control: InternalCaptureControl) -> Result<(), Error> {
        let s = &self.shared;
        {
            let mut encoder = s.encoder.lock().unwrap();
            if encoder.is_none() {
                // H.264 needs even dimensions; the encoder pads to this size.
                let (w, h) = ((frame.width() + 1) & !1, (frame.height() + 1) & !1);
                let audio_settings = AudioSettingsBuilder::new()
                    .sample_rate(audio::SAMPLE_RATE)
                    .channel_count(audio::CHANNELS as u32)
                    .disabled(s.settings.audio == AudioMode::Off);
                let video = VideoSettingsBuilder::new(w, h)
                    .sub_type(VideoSettingsSubType::H264)
                    .frame_rate(s.settings.fps)
                    .bitrate(s.settings.bitrate_mbps * 1_000_000);
                *encoder = Some(VideoEncoder::new(video, audio_settings, ContainerSettingsBuilder::new(), &s.path)?);
                *s.started.lock().unwrap() = Some(Instant::now());
            }
            if let Err(e) = encoder.as_mut().unwrap().send_frame(frame) {
                *s.error.lock().unwrap() = Some(e.to_string());
                return Err(e.into());
            }
        }

        let mut last = s.last_detect_copy.lock().unwrap();
        if last.is_none_or(|t| t.elapsed() >= DETECT_FRAME_INTERVAL) {
            *last = Some(Instant::now());
            let buffer = frame.buffer()?;
            let (width, height) = (buffer.width(), buffer.height());
            let mut scratch = Vec::new();
            let mut rgba = buffer.as_nopadding_buffer(&mut scratch).to_vec();
            // Captured as BGRA for the encoder; the detector expects RGBA.
            for px in rgba.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
            let mut sink = s.sink.lock().unwrap();
            sink.latest = Some(FrameData { width, height, rgba });
            sink.generation = sink.generation.wrapping_add(1);
        }
        Ok(())
    }
}

pub struct Recorder {
    control: Option<CaptureControl<Handler, Error>>,
    audio: Option<AudioCapture>,
    shared: Arc<Shared>,
}

impl Recorder {
    /// Starts recording `window` into `path` (an .mp4).
    pub fn start(window: Window, path: &Path, settings: RecordSettings) -> Result<Self> {
        let shared = Arc::new(Shared {
            path: path.to_path_buf(),
            settings,
            encoder: Mutex::new(None),
            started: Arc::new(Mutex::new(None)),
            sink: Arc::new(Mutex::new(FrameSink::default())),
            last_detect_copy: Mutex::new(None),
            error: Mutex::new(None),
        });

        let pid = window.process_id().map_err(|e| anyhow!("讀取遊戲程序 ID 失敗: {e}"))?;
        let audio_shared = shared.clone();
        let audio = AudioCapture::start(settings.audio, pid, shared.started.clone(), move |pcm| {
            if let Some(enc) = audio_shared.encoder.lock().unwrap().as_mut() {
                let _ = enc.send_audio_buffer(pcm, 0);
            }
        })?;

        let capture_settings = Settings::new(
            window,
            CursorCaptureSettings::WithoutCursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Custom(Duration::from_secs_f64(1.0 / settings.fps as f64)),
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            shared.clone(),
        );
        let control = Handler::start_free_threaded(capture_settings).map_err(|e| anyhow!("啟動錄影失敗: {e}"))?;
        Ok(Self { control: Some(control), audio, shared })
    }

    /// Latest frames for the detector.
    pub fn frames(&self) -> SharedSink {
        self.shared.sink.clone()
    }

    /// Seconds since the recording's first frame: the position in the video.
    pub fn elapsed(&self) -> Option<f64> {
        self.shared.started.lock().unwrap().map(|t| t.elapsed().as_secs_f64())
    }

    /// An encoding error, if the recording has failed.
    pub fn error(&self) -> Option<String> {
        self.shared.error.lock().unwrap().clone()
    }

    /// Stops capturing and finalizes the file.
    pub fn stop(mut self) -> Result<PathBuf> {
        if let Some(audio) = self.audio.take() {
            audio.stop();
        }
        if let Some(control) = self.control.take() {
            let _ = control.stop();
        }
        let encoder = self.shared.encoder.lock().unwrap().take();
        match encoder {
            Some(enc) => enc.finish().map_err(|e| anyhow!("錄影檔收尾失敗: {e}"))?,
            None => return Err(anyhow!("沒有錄到任何畫面(遊戲視窗是否被最小化?)")),
        }
        Ok(self.shared.path.clone())
    }
}
