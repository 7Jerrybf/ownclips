// Audio for the built-in recorder: captures one or two WASAPI sources and
// writes them, mixed, as raw 48 kHz / stereo / 16-bit PCM to a file that is
// muxed with the video when the recording ends.
//
// Every WASAPI packet carries the performance-counter time of its first
// sample, so each packet is placed at its true position relative to
// `start` (qpc.rs). That keeps audio in sync with the video even across the
// silences in which loopback capture delivers no packets at all. Packets
// that follow on from the previous one are joined seamlessly; a jump in
// time (after silence, or clock drift) is honoured by inserting silence.
// The writer stays a fixed delay behind real time so late packets still
// land where they belong.
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use wasapi::{AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

use crate::qpc;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

/// How far behind real time the writer runs.
const WRITE_DELAY_TICKS: u64 = 4_000_000; // 400 ms
const WRITE_TICK: Duration = Duration::from_millis(50);
/// Packets this close to where the previous one ended are treated as
/// contiguous (WASAPI timestamps jitter by a few ms).
const CONTIGUOUS_TOLERANCE: i64 = SAMPLE_RATE as i64 / 50; // 20 ms

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

/// One source's not-yet-written samples, starting at absolute frame `start`
/// (frames counted from the recording's `start` time).
#[derive(Default)]
struct Pending {
    start: i64,
    samples: VecDeque<f32>,
}

impl Pending {
    fn end(&self) -> i64 {
        self.start + (self.samples.len() / CHANNELS) as i64
    }

    /// Adds a packet whose first frame belongs at `pos`.
    fn push(&mut self, pos: i64, data: &[f32]) {
        if self.samples.is_empty() {
            self.start = pos;
        } else {
            let gap = pos - self.end();
            if gap > CONTIGUOUS_TOLERANCE {
                self.samples.extend(std::iter::repeat_n(0.0, gap as usize * CHANNELS));
            }
            // Smaller gaps and overlaps are timestamp jitter: append as-is.
        }
        self.samples.extend(data);
    }
}

pub struct AudioRecorder {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    writer: Option<JoinHandle<Result<()>>>,
}

impl AudioRecorder {
    /// Starts recording `mode` into `out` (raw s16le), with sample 0 at
    /// performance-counter time `start`.
    pub fn start(mode: AudioMode, game_pid: u32, start: u64, out: &Path) -> Result<Option<Self>> {
        let sources = match mode {
            AudioMode::Off => return Ok(None),
            AudioMode::Game => vec![Source::Process(game_pid)],
            AudioMode::System => vec![Source::SystemLoopback],
            AudioMode::GameAndMic => vec![Source::Process(game_pid), Source::Microphone],
        };

        let stop = Arc::new(AtomicBool::new(false));
        let mut threads = Vec::new();
        let mut pendings = Vec::new();
        for source in sources {
            let pending: Arc<Mutex<Pending>> = Arc::default();
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let (stop2, pending2) = (stop.clone(), pending.clone());
            threads.push(thread::spawn(move || {
                if let Err(e) = capture_source(source, start, &pending2, &stop2, &ready_tx) {
                    let _ = ready_tx.send(Err(e.to_string()));
                }
            }));
            // Surface setup errors (no microphone, bad pid...) right away.
            match ready_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(())) => pendings.push(pending),
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

        let file = BufWriter::with_capacity(1 << 20, File::create(out)?);
        let stop2 = stop.clone();
        let writer = thread::spawn(move || write_loop(pendings, start, file, &stop2));
        Ok(Some(Self { stop, threads, writer: Some(writer) }))
    }

    /// Stops capturing and writes everything up to now.
    pub fn stop(mut self) -> Result<()> {
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        match self.writer.take().map(|w| w.join()) {
            Some(Ok(r)) => r,
            Some(Err(_)) => Err(anyhow!("聲音寫入執行緒當掉了")),
            None => Ok(()),
        }
    }
}

fn frames_since(start: u64, t: u64) -> i64 {
    ((t as i128 - start as i128) * SAMPLE_RATE as i128 / qpc::TICKS_PER_SEC as i128) as i64
}

fn capture_source(
    source: Source,
    start: u64,
    pending: &Mutex<Pending>,
    stop: &AtomicBool,
    ready: &std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<()> {
    wasapi::initialize_mta().ok().map_err(|e| anyhow!("COM 初始化失敗: {e}"))?;
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
            raw.clear();
            let info = capture.read_from_device_to_deque(&mut raw)?;
            let frames = raw.len() / (4 * CHANNELS);
            if frames == 0 {
                continue;
            }
            // Some loopback clients report no timestamp; then assume the
            // packet just finished playing.
            let t = if info.timestamp > 0 && !info.flags.timestamp_error {
                info.timestamp
            } else {
                qpc::now().saturating_sub(frames as u64 * qpc::TICKS_PER_SEC as u64 / SAMPLE_RATE as u64)
            };
            let data: Vec<f32> = if info.flags.silent {
                vec![0.0; frames * CHANNELS]
            } else {
                raw.make_contiguous()
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect()
            };
            pending.lock().unwrap().push(frames_since(start, t), &data);
        }
    }
    let _ = client.stop_stream();
    Ok(())
}

fn write_loop(pendings: Vec<Arc<Mutex<Pending>>>, start: u64, mut out: BufWriter<File>, stop: &AtomicBool) -> Result<()> {
    let mut written: i64 = 0;
    let mut mix: Vec<f32> = Vec::new();
    let mut pcm: Vec<u8> = Vec::new();
    loop {
        let stopping = stop.load(Ordering::Relaxed);
        if !stopping {
            thread::sleep(WRITE_TICK);
        }
        // When stopping, flush everything that has arrived.
        let horizon = if stopping { qpc::now() } else { qpc::now().saturating_sub(WRITE_DELAY_TICKS) };
        let target = frames_since(start, horizon);
        if target > written {
            let n = (target - written) as usize;
            mix.clear();
            mix.resize(n * CHANNELS, 0.0);
            for p in &pendings {
                let mut p = p.lock().unwrap();
                // Drop anything that arrived too late to be placed.
                if p.start < written {
                    let late = ((written - p.start) as usize * CHANNELS).min(p.samples.len());
                    p.samples.drain(..late);
                    p.start = written;
                }
                let offset = (p.start - written).max(0) as usize;
                if offset < n {
                    let take = ((n - offset) * CHANNELS).min(p.samples.len());
                    for (m, s) in mix[offset * CHANNELS..].iter_mut().zip(p.samples.drain(..take)) {
                        *m += s;
                    }
                    p.start += (take / CHANNELS) as i64;
                }
            }
            pcm.clear();
            pcm.extend(mix.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes()));
            out.write_all(&pcm)?;
            written = target;
        }
        if stopping {
            out.flush()?;
            return Ok(());
        }
    }
}
