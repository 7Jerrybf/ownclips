// The Windows performance counter in 100 ns units: the clock that both
// Windows.Graphics.Capture frame times and WASAPI packet timestamps use, so
// audio and video can be lined up exactly.
use std::sync::OnceLock;

#[link(name = "kernel32")]
extern "system" {
    fn QueryPerformanceCounter(count: *mut i64) -> i32;
    fn QueryPerformanceFrequency(freq: *mut i64) -> i32;
}

/// Current time in 100 ns ticks.
pub fn now() -> u64 {
    static FREQ: OnceLock<i64> = OnceLock::new();
    let freq = *FREQ.get_or_init(|| {
        let mut f = 0;
        unsafe { QueryPerformanceFrequency(&mut f) };
        f.max(1)
    });
    let mut c = 0;
    unsafe { QueryPerformanceCounter(&mut c) };
    (c as i128 * 10_000_000 / freq as i128) as u64
}

pub const TICKS_PER_SEC: f64 = 10_000_000.0;
