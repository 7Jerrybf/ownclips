// Audio for the built-in recorder: captures one or two WASAPI sources and
// mixes them into the 48 kHz / stereo / 16-bit PCM the video encoder takes.
//
// Loopback sources deliver nothing while nothing is playing, so the
// encoder's audio clock cannot simply follow the packets that arrive. A
// mixer thread instead emits audio on the wall clock (padding with silence),
// running a fixed delay behind real time so each source has slack to deliver
// its packets; a source that falls far behind or runs ahead is trimmed so
// latency stays bounded.
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use wasapi::{AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

/// How far behind real time the mixer runs.
const MIX_DELAY: Duration = Duration::from_millis(150);
/// A source buffering more than this is trimmed back (it ran ahead).
const MAX_BUFFERED: Duration = Duration::from_millis(500);
const MIX_TICK: Duration = Duration::from_millis(20);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioMode {
    /// Only the game's own audio (per-process loopback).
    Game,
    /// Everything the speakers/headphones play.
    System,
    /// The game's audio plus the default microphone.
    GameAndMic,
    /// No audio track.
    Off,
}

#[derive(Clone, Copy)]
enum Source {
    Process(u32),
    SystemLoopback,
    Microphone,
}

type SampleBuf = Arc<Mutex<VecDeque<f32>>>;

pub struct AudioCapture {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl AudioCapture {
    /// Starts capturing for `mode`. Mixed PCM is handed to `sink` in small
    /// chunks once `start_at` has been set (the moment the first video frame
    /// was encoded, so both tracks share time zero).
    pub fn start(
        mode: AudioMode,
        game_pid: u32,
        start_at: Arc<Mutex<Option<Instant>>>,
        sink: impl FnMut(&[u8]) + Send + 'static,
    ) -> Result<Option<Self>> {
        let sources = match mode {
            AudioMode::Off => return Ok(None),
            AudioMode::Game => vec![Source::Process(game_pid)],
            AudioMode::System => vec![Source::SystemLoopback],
            AudioMode::GameAndMic => vec![Source::Process(game_pid), Source::Microphone],
        };

        let stop = Arc::new(AtomicBool::new(false));
        let mut threads = Vec::new();
        let mut bufs = Vec::new();
        for source in sources {
            let buf: SampleBuf = Arc::default();
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let (stop2, buf2) = (stop.clone(), buf.clone());
            threads.push(thread::spawn(move || {
                if let Err(e) = capture_source(source, &buf2, &stop2, &ready_tx) {
                    let _ = ready_tx.send(Err(e.to_string()));
                }
            }));
            // Surface setup errors (no microphone, bad pid...) right away.
            match ready_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(())) => bufs.push(buf),
                Ok(Err(e)) => {
                    stop.store(true, Ordering::Relaxed);
                    return Err(anyhow!("啟動聲音擷取失敗: {e}"));
                }
                Err(_) => {
                    stop.store(true, Ordering::Relaxed);
                    return Err(anyhow!("啟動聲音擷取逾時"));
                }
            }
        }

        let stop2 = stop.clone();
        threads.push(thread::spawn(move || mix_loop(bufs, start_at, &stop2, sink)));
        Ok(Some(Self { stop, threads }))
    }

    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads {
            let _ = t.join();
        }
    }
}

fn capture_source(
    source: Source,
    buf: &SampleBuf,
    stop: &AtomicBool,
    ready: &std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<()> {
    wasapi::initialize_mta()
        .ok()
        .map_err(|e| anyhow!("COM 初始化失敗: {e}"))?;
    let format = WaveFormat::new(32, 32, &SampleType::Float, SAMPLE_RATE as usize, CHANNELS, None);
    let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 0 };

    let mut client = match source {
        Source::Process(pid) => AudioClient::new_application_loopback_client(pid, true)?,
        Source::SystemLoopback => DeviceEnumerator::new()?.get_default_device(&Direction::Render)?.get_iaudioclient()?,
        Source::Microphone => DeviceEnumerator::new()?.get_default_device(&Direction::Capture)?.get_iaudioclient()?,
    };
    // Capturing from a render device is a loopback capture.
    client.initialize_client(&format, &Direction::Capture, &mode)?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    client.start_stream()?;
    let _ = ready.send(Ok(()));

    let mut raw: VecDeque<u8> = VecDeque::new();
    while !stop.load(Ordering::Relaxed) {
        // Times out regularly while nothing is playing; that is normal.
        if event.wait_for_event(200).is_err() {
            continue;
        }
        while capture.get_next_packet_size()?.unwrap_or(0) > 0 {
            capture.read_from_device_to_deque(&mut raw)?;
        }
        let whole = raw.len() / 4 * 4;
        let mut out = buf.lock().unwrap();
        out.extend(raw.drain(..whole).collect::<Vec<u8>>().chunks_exact(4).map(|b| {
            f32::from_le_bytes([b[0], b[1], b[2], b[3]])
        }));
    }
    let _ = client.stop_stream();
    Ok(())
}

fn mix_loop(
    bufs: Vec<SampleBuf>,
    start_at: Arc<Mutex<Option<Instant>>>,
    stop: &AtomicBool,
    mut sink: impl FnMut(&[u8]),
) {
    let samples_for = |d: Duration| (d.as_secs_f64() * SAMPLE_RATE as f64) as u64;
    let max_buffered = samples_for(MAX_BUFFERED) as usize * CHANNELS;
    let mut sent_frames: u64 = 0;
    let mut mix: Vec<f32> = Vec::new();
    let mut pcm: Vec<u8> = Vec::new();

    while !stop.load(Ordering::Relaxed) {
        thread::sleep(MIX_TICK);
        let Some(start) = *start_at.lock().unwrap() else {
            // Not recording video yet: drop what arrives so it can't pile up.
            for b in &bufs {
                b.lock().unwrap().clear();
            }
            continue;
        };
        let target = samples_for(start.elapsed().saturating_sub(MIX_DELAY));
        if target <= sent_frames {
            continue;
        }
        let frames = (target - sent_frames) as usize;
        mix.clear();
        mix.resize(frames * CHANNELS, 0.0);
        for b in &bufs {
            let mut b = b.lock().unwrap();
            if b.len() > max_buffered + mix.len() {
                let excess = b.len() - max_buffered;
                b.drain(..excess / CHANNELS * CHANNELS);
            }
            let n = mix.len().min(b.len());
            for (m, s) in mix.iter_mut().zip(b.drain(..n)) {
                *m += s;
            }
        }
        pcm.clear();
        pcm.extend(mix.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes()));
        sink(&pcm);
        sent_frames = target;
    }
}
