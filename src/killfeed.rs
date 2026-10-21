// Pure-color self-kill detection on a cropped killfeed image.
//
// Valorant draws a thin yellow frame (top and bottom edge lines) around the
// local player's portrait in every killfeed row they appear in. The local
// player is on the left (killer side) of the row for a self-kill and on the
// right (victim side) when they died. This replaced the earlier OCR rule
// ("我" as the first word) because ocrs only recognizes Latin script and can
// never output "我".
//
// Tuned against the killfeed_samples/ screenshots collected with
// watch-killfeed: requiring a *pair* of thin horizontal lines one row-height
// apart rules out yellow blobs in agent portraits (e.g. Killjoy's jacket)
// and yellowish map backgrounds.
use std::time::{Duration, Instant};

use image::RgbImage;

/// Killfeed row height as a fraction of the full game window height
/// (measured: 44px at 1440p).
pub const ROW_HEIGHT_FRAC: f32 = 0.0306;

#[derive(Clone, Copy, PartialEq)]
enum Px {
    Yellow,
    Bar, // the teal (ally) / red (enemy) row background
    Other,
}

fn classify(p: &image::Rgb<u8>) -> Px {
    let [r, g, b] = p.0.map(i32::from);
    // The self frame is ~(224,232,112), warmer where it blends into a red
    // row; near-white pale yellows (~254,250,153) are map backgrounds.
    if r > 190 && g > 190 && b < 160 && (r - g).abs() < 35 && r - b > 70 && r < 246 && g < 246 {
        Px::Yellow
    } else if g > 150 && b > 120 && r < 175 && g - r > 35 && b <= g {
        // Teal (ally) row, including its lighter end next to the portrait;
        // b <= g keeps light-blue UI backgrounds out.
        Px::Bar
    } else if r > 180 && g < 130 && b < 130 && r - g > 80 {
        Px::Bar
    } else {
        Px::Other
    }
}

/// Horizontal runs of yellow on one scanline (gaps up to 2px tolerated),
/// as inclusive `(start, end)` x ranges at least `min_len` long.
fn yellow_runs(row: &[Px], min_len: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut last = 0;
    for x in 0..=row.len() {
        if x < row.len() && row[x] == Px::Yellow {
            start.get_or_insert(x);
            last = x;
        } else if let Some(s) = start {
            if x == row.len() || x - last > 2 {
                if last - s + 1 >= min_len {
                    out.push((s, last));
                }
                start = None;
            }
        }
    }
    out
}

/// Rows of one kind (local player as killer, or as victim) on screen.
#[derive(Default, Clone, Copy)]
pub struct SelfRows {
    pub count: usize,
    /// Whether one of those rows is the bottom (newest) row of the feed.
    pub newest: bool,
}

pub struct KillfeedScan {
    pub kills: SelfRows,
    pub deaths: SelfRows,
    /// Rows where the local player assisted: their small assist icon (left
    /// of the killer's portrait) gets the same yellow frame, half a row tall.
    pub assists: SelfRows,
    /// Yellow pixels outside the recognized rows; frames with a lot of it
    /// are saved for study.
    pub stray_yellow: usize,
}

/// Finds the killfeed rows in `img` where the local player is the killer
/// or the victim. `row_h` is the expected row height in `img` pixels.
pub fn scan(img: &RgbImage, row_h: f32) -> KillfeedScan {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let grid: Vec<Vec<Px>> = (0..h)
        .map(|y| (0..w).map(|x| classify(img.get_pixel(x as u32, y as u32))).collect())
        .collect();

    let yellow_frac = |y: isize, a: usize, b: usize| -> f32 {
        if y < 0 || y as usize >= h {
            return 0.0;
        }
        let n = (a..=b).filter(|&x| grid[y as usize][x] == Px::Yellow).count();
        n as f32 / (b - a + 1) as f32
    };

    // Thin yellow lines: a long run whose surroundings 4px above or below
    // are mostly not yellow (a frame edge, not a filled yellow area).
    let min_len = row_h.max(8.0) as usize;
    let mut lines = Vec::new();
    for y in 0..h {
        for (a, b) in yellow_runs(&grid[y], min_len) {
            let yi = y as isize;
            if yellow_frac(yi - 4, a, b).min(yellow_frac(yi + 4, a, b)) < 0.3 {
                lines.push((y, a, b));
            }
        }
    }

    let bar_count = |y: usize, xs: std::ops::Range<usize>| xs.filter(|&x| grid[y][x] == Px::Bar).count() as f32;

    // Pair a top and bottom frame line one row-height apart. The frame sits
    // on the left end of the row (little bar to its left, lots to its right)
    // when the local player is the killer, and on the right end when they
    // are the victim.
    let mut kill_rows: Vec<(usize, usize)> = Vec::new();
    let mut death_rows: Vec<(usize, usize)> = Vec::new();
    for &(y0, a0, b0) in &lines {
        for &(y1, a1, b1) in &lines {
            let dy = y1 as f32 - y0 as f32;
            if dy < 0.75 * row_h || dy > 1.25 * row_h {
                continue;
            }
            let overlap = b0.min(b1) as isize - a0.max(a1) as isize;
            if (overlap as f32) < 0.5 * (b0 - a0).min(b1 - a1) as f32 {
                continue;
            }
            let ym = (y0 + y1) / 2;
            let (a, b) = (a0.min(a1), b0.max(b1));
            let left = bar_count(ym, 0..a);
            let right = bar_count(ym, b + 1..w);
            let rows = if left < 2.0 * row_h && right > 2.0 * row_h {
                &mut kill_rows
            } else if right < 2.0 * row_h && left > 2.0 * row_h {
                &mut death_rows
            } else {
                continue;
            };
            if !rows.iter().any(|&(r, _)| (r as f32 - y0 as f32).abs() < 0.5 * row_h) {
                rows.push((y0, y1));
            }
        }
    }

    // Assist icons: a thin yellow frame about half a row tall, directly
    // left of a row (the killer's portrait sits between it and the bar).
    let mut small_lines = Vec::new();
    for y in 0..h {
        for (a, b) in yellow_runs(&grid[y], (0.3 * row_h) as usize) {
            let yi = y as isize;
            if (b - a) as f32 <= 1.3 * row_h && yellow_frac(yi - 4, a, b).min(yellow_frac(yi + 4, a, b)) < 0.3 {
                small_lines.push((y, a, b));
            }
        }
    }
    let in_rows = |rows: &[(usize, usize)], y0: usize, y1: usize| {
        rows.iter().any(|&(r0, r1)| y1 + 2 >= r0 && y0 <= r1 + 2)
    };
    let mut assist_rows: Vec<(usize, usize)> = Vec::new();
    for &(y0, a0, b0) in &small_lines {
        for &(y1, a1, b1) in &small_lines {
            let dy = y1 as f32 - y0 as f32;
            if dy < 0.4 * row_h || dy > 0.7 * row_h {
                continue;
            }
            let overlap = b0.min(b1) as isize - a0.max(a1) as isize;
            if (overlap as f32) < 0.5 * (b0 - a0).min(b1 - a1) as f32 {
                continue;
            }
            // Not part of the player's own kill/death frame.
            if in_rows(&kill_rows, y0, y1) || in_rows(&death_rows, y0, y1) {
                continue;
            }
            let ym = (y0 + y1) / 2;
            let (a, b) = (a0.min(a1), b0.max(b1));
            let near = (b + 1)..(b + 1 + (4.0 * row_h) as usize).min(w);
            if bar_count(ym, 0..a) < row_h
                && bar_count(ym, near) > 0.5 * row_h
                && !assist_rows.iter().any(|&(r, _)| (r as f32 - y0 as f32).abs() < 0.5 * row_h)
            {
                assist_rows.push((y0, y1));
            }
        }
    }

    // New rows are appended at the bottom of the feed, so the lowest
    // scanline that is mostly row background marks the newest row.
    let feed_bottom = (0..h).rev().find(|&y| bar_count(y, 0..w) >= 2.0 * row_h);
    let summarize = |rows: &[(usize, usize)]| SelfRows {
        count: rows.len(),
        newest: feed_bottom.is_some_and(|fb| rows.iter().any(|&(_, y1)| y1 as f32 + 0.5 * row_h >= fb as f32)),
    };

    let margin = (0.3 * row_h) as usize;
    let in_self_row = |y: usize| {
        kill_rows.iter().chain(&death_rows).chain(&assist_rows).any(|&(y0, y1)| y + margin >= y0 && y <= y1 + margin)
    };
    let stray_yellow = (0..h)
        .filter(|&y| !in_self_row(y))
        .map(|y| grid[y].iter().filter(|&&p| p == Px::Yellow).count())
        .sum();

    KillfeedScan {
        kills: summarize(&kill_rows),
        deaths: summarize(&death_rows),
        assists: summarize(&assist_rows),
        stray_yellow,
    }
}

/// Turns per-poll row counts into discrete events: an event is counted
/// whenever the number of rows rises above the recent maximum, which
/// tolerates rows lingering for several seconds, shifting position as new
/// ones arrive, and the occasional poll where detection misses.
///
/// A row hidden for longer (e.g. under another app's overlay) would still
/// re-count once it reappears, by then pushed up the feed. New rows always
/// enter at the bottom, so an increase from a row that is not the newest is
/// only trusted when nothing was counted within a row's on-screen lifetime.
#[derive(Default)]
pub struct RowCounter {
    recent: Vec<(usize, Instant)>,
    last_counted: Option<Instant>,
}

const COUNT_MEMORY: Duration = Duration::from_millis(1500);
const ROW_LIFETIME: Duration = Duration::from_secs(7);

impl RowCounter {
    /// Feeds one poll's rows; returns how many new events they represent.
    pub fn update(&mut self, rows: SelfRows) -> usize {
        self.recent.retain(|(_, t)| t.elapsed() < COUNT_MEMORY);
        let prev = self.recent.iter().map(|(c, _)| *c).max().unwrap_or(0);
        self.recent.push((rows.count, Instant::now()));
        if rows.count <= prev {
            return 0;
        }
        let recently_counted = self.last_counted.is_some_and(|t| t.elapsed() < ROW_LIFETIME);
        if !rows.newest && recently_counted {
            return 0; // an already-counted row resurfacing higher up the feed
        }
        self.last_counted = Some(Instant::now());
        rows.count - prev
    }
}
