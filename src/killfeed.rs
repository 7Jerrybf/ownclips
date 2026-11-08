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
//
// The agent art often covers most of those lines, so for deaths a second
// check looks at the portrait's background instead: lime for the local
// player where everyone else's is the row's team color. Rows either check
// finds are counted once.
use image::RgbImage;

/// Killfeed row height as a fraction of the full game window height
/// (measured: 44px at 1440p).
pub const ROW_HEIGHT_FRAC: f32 = 0.0306;

#[derive(Clone, Copy, PartialEq)]
enum Px {
    Yellow,
    // Row backgrounds: ally rows are teal, enemy rows red.
    Teal,
    Red,
    Other,
}

impl Px {
    fn is_bar(self) -> bool {
        matches!(self, Px::Teal | Px::Red)
    }
}

/// The local player's portrait background, ~(230,235,118): greener than
/// the warm yellows of map backgrounds and agent art.
fn is_lime(p: &image::Rgb<u8>) -> bool {
    let [r, g, b] = p.0.map(i32::from);
    r > 170 && g > 185 && b < 175 && g + 8 >= r && g - b > 55
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
        Px::Teal
    } else if r > 180 && g < 130 && b < 130 && r - g > 80 {
        Px::Red
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

/// Where a killfeed row's killer team color gives way to the victim's.
/// Rows are right-aligned, so this is fixed by what is right of it (the
/// victim's name and portrait): it stays put while the row moves up the
/// feed, and differs between most rows. (Measured stable to a pixel or two;
/// the bar's left end, by contrast, wanders with the portrait art.)
pub type RowSig = usize;

/// Rows of one kind (local player as killer, or as victim) on screen.
#[derive(Default, Clone)]
pub struct SelfRows {
    pub count: usize,
    /// Each row's top and bottom scanline.
    pub rows: Vec<(usize, usize)>,
    /// Each row's signature (same order as `rows`).
    pub sigs: Vec<RowSig>,
    /// Whether one of those rows is the bottom (newest) row of the feed.
    pub newest: bool,
    /// Detection image row height, for comparing signatures.
    pub row_h: f32,
}

pub struct KillfeedScan {
    pub kills: SelfRows,
    pub deaths: SelfRows,
    /// Rows where the local player assisted: their small assist icon (left
    /// of the killer's portrait) gets the same yellow frame, half a row tall.
    pub assists: SelfRows,
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
    let thin = |y: isize, a: usize, b: usize| yellow_frac(y - 4, a, b).min(yellow_frac(y + 4, a, b)) < 0.3;
    let min_len = row_h.max(8.0) as usize;
    let mut lines = Vec::new();
    for y in 0..h {
        for (a, b) in yellow_runs(&grid[y], min_len) {
            let yi = y as isize;
            if thin(yi, a, b) {
                lines.push((y, a, b));
            }
        }
    }

    let bar_count = |y: usize, xs: std::ops::Range<usize>| xs.filter(|&x| grid[y][x].is_bar()).count() as f32;
    let both_teams = |y: usize| has_both_teams(&grid[y], row_h);

    // Scanlines that are mostly row bar, and runs of them (killfeed rows).
    let bar_line: Vec<bool> = (0..h).map(|y| bar_count(y, 0..w) >= 2.0 * row_h).collect();
    let mut bar_runs: Vec<(usize, usize)> = Vec::new();
    for y in (0..h).filter(|&y| bar_line[y]) {
        match bar_runs.last_mut() {
            Some((_, end)) if *end + 1 == y => *end = y,
            _ => bar_runs.push((y, y)),
        }
    }
    // A self frame's edges are the top and bottom scanlines of its row, not
    // the gaps between rows (which a bright yellow flash behind the
    // see-through rows turns into yellow lines too).
    let on_row = |y: usize, inward: usize| bar_line[y] || bar_line[inward];
    let fits_row = |y0: usize, y1: usize| on_row(y0, y0 + 1) && on_row(y1, y1 - 1);

    // Pair each frame line with the frame's other edge one row-height below
    // or above it. That edge is often broken up by the portrait art drawn
    // over it, so it only has to be mostly yellow across the same span, with
    // an unbroken stretch at least half as long. The
    // frame sits on the left end of the row (little bar to its left, lots
    // to its right) when the local player is the killer, and on the right
    // end when they are the victim.
    // Real frames measure 0.93-1.02 rows; the gold-edged killer card on the
    // death screen pairs up at other heights.
    let (near, far) = ((0.88 * row_h).ceil() as isize, (1.12 * row_h) as isize);
    let mut kill_rows: Vec<(usize, usize)> = Vec::new();
    let mut death_rows: Vec<(usize, usize)> = Vec::new();
    for &(y, a, b) in &lines {
        let yi = y as isize;
        let partner = (near..=far)
            .flat_map(|d| [yi + d, yi - d])
            .find(|&p| {
                p >= 0
                    && (p as usize) < h
                    && yellow_frac(p, a, b) >= 0.6
                    && thin(p, a, b)
                    && yellow_runs(&grid[p as usize][a..=b], (b - a + 1) / 2).len() > 0
            });
        if let Some(p) = partner {
            let (y0, y1) = if p > yi { (y, p as usize) } else { (p as usize, y) };
            let ym = (y0 + y1) / 2;
            if !fits_row(y0, y1) || !both_teams(ym) {
                continue;
            }
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
            if (b - a) as f32 <= 1.3 * row_h && thin(yi, a, b) {
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
                && both_teams(ym)
                && !assist_rows.iter().any(|&(r, _)| (r as f32 - y0 as f32).abs() < 0.5 * row_h)
            {
                assist_rows.push((y0, y1));
            }
        }
    }

    for (y0, y1) in lime_death_rows(img, &grid, &bar_runs, row_h) {
        if !in_rows(&death_rows, y0, y1) {
            death_rows.push((y0, y1));
        }
    }

    // New rows are appended at the bottom of the feed, so the lowest
    // scanline that is mostly row background marks the newest row.
    let feed_bottom = (0..h).rev().find(|&y| bar_count(y, 0..w) >= 2.0 * row_h);
    // The local player's team is teal: on the killer side for kills and
    // assists, on the victim side for deaths.
    // Rows whose signature can't be measured (not two-toned the right way
    // round) aren't the local player's.
    let summarize = |rows: &[(usize, usize)], ally_left: bool| {
        let (rows, sigs): (Vec<_>, Vec<_>) = rows
            .iter()
            .filter_map(|&(y0, y1)| Some(((y0, y1), row_sig(&grid, (y0 + y1) / 2, row_h, ally_left)?)))
            .unzip();
        SelfRows {
            count: rows.len(),
            newest: feed_bottom.is_some_and(|fb| rows.iter().any(|&(_, y1)| y1 as f32 + 0.5 * row_h >= fb as f32)),
            rows,
            sigs,
            row_h,
        }
    };

    KillfeedScan {
        kills: summarize(&kill_rows, true),
        deaths: summarize(&death_rows, false),
        assists: summarize(&assist_rows, true),
    }
}

/// Measures the signature (see `RowSig`) of the row centred on scanline `ym`, from the
/// middle half of its height (text and icons are white, so columns are
/// judged by majority). `ally_left`: teal on the left, red on the right.
fn row_sig(grid: &[Vec<Px>], ym: usize, row_h: f32, ally_left: bool) -> Option<RowSig> {
    let h = grid.len();
    let w = grid.first()?.len();
    let half = (0.25 * row_h).max(1.0) as usize;
    let band = ym.saturating_sub(half)..(ym + half + 1).min(h);
    let n = band.len() as f32;
    let (mut teal, mut red) = (vec![0f32; w], vec![0f32; w]);
    for y in band {
        for x in 0..w {
            match grid[y][x] {
                Px::Teal => teal[x] += 1.0,
                Px::Red => red[x] += 1.0,
                _ => {}
            }
        }
    }
    // The bar runs from its left end to the region's right edge; a column
    // counts as bar when most of the band is bar color.
    let is_bar = |x: usize| teal[x] + red[x] >= 0.5 * n;
    // The bar's left end: the first bar column with the bar continuing for
    // a while after it (not a stray teal patch of map).
    let run = (row_h as usize).max(4);
    let left = (0..w.saturating_sub(run)).find(|&x| (x..x + run).filter(|&i| is_bar(i)).count() * 4 >= run * 3)?;
    // Split where the most columns agree: killer color left, victim right.
    let (l_px, r_px) = if ally_left { (&teal, &red) } else { (&red, &teal) };
    let mut score: f32 = (left..w).map(|x| r_px[x]).sum();
    let (mut best, mut split) = (score, left);
    for x in left..w {
        score += l_px[x] - r_px[x];
        if score > best {
            best = score;
            split = x + 1;
        }
    }
    // A row with the local player in it is two-toned: mostly the killer's
    // color left of the split and the victim's right of it. Rows between
    // two other players (all red in Deathmatch) that a yellow-ish portrait
    // made look like ours are not.
    let side = |xs: std::ops::Range<usize>| -> (f32, f32) {
        xs.fold((0.0, 0.0), |(a, b), x| (a + l_px[x], b + r_px[x]))
    };
    let ((l_own, l_other), (r_other, r_own)) = (side(left..split), side(split..w));
    let min = 0.5 * row_h * n;
    if l_own < min || r_own < min || l_own < 2.0 * l_other || r_own < 2.0 * r_other {
        return None;
    }
    Some(split)
}

/// Every row with the local player in it pairs an ally with an enemy, so
/// its scanlines have both colors; an all-teal ability dome or sky does
/// not. One side can be short: the local player's name is "我".
fn has_both_teams(line: &[Px], row_h: f32) -> bool {
    let count = |px: Px| line.iter().filter(|&&p| p == px).count() as f32;
    count(Px::Teal) >= 0.3 * row_h && count(Px::Red) >= 0.3 * row_h
}

/// The lime-background check for death rows: the victim's portrait box is
/// the right end of the row, where the bar stops and (for the local
/// player) lime fills what the agent art leaves visible. The killer side is
/// left out: lime-ish map backgrounds next to a row's start fool it.
fn lime_death_rows(img: &RgbImage, grid: &[Vec<Px>], bar_runs: &[(usize, usize)], row_h: f32) -> Vec<(usize, usize)> {
    let w = grid.first().map_or(0, Vec::len);
    let mut rows = Vec::new();
    for &(y0, y1) in bar_runs {
        // One row tall (a teal map background can merge rows).
        let n = y1 + 1 - y0;
        if (n as f32) < 0.5 * row_h || n as f32 > 1.3 * row_h {
            continue;
        }
        if !has_both_teams(&grid[(y0 + y1) / 2], row_h) {
            continue;
        }
        let covered = |x: usize| (y0..=y1).filter(|&y| grid[y][x].is_bar()).count() * 2 >= n;
        let Some(br) = (0..w).rev().find(|&x| covered(x)) else { continue };
        // The portrait box (1.8 rows wide) runs to the region's edge.
        let gap = (w - 1 - br) as f32;
        if gap < 1.5 * row_h || gap > 2.1 * row_h {
            continue;
        }
        let lime: usize =
            (y0..=y1).map(|y| (br + 1..w).filter(|&x| is_lime(img.get_pixel(x as u32, y as u32))).count()).sum();
        // Measured 12-14% of the box (the art covers the rest).
        if lime as f32 >= 0.06 * (n * (w - 1 - br)) as f32 {
            rows.push((y0, y1));
        }
    }
    rows
}

/// Turns the rows seen on each poll into discrete events by following
/// each row from poll to poll. A row on screen is the same as a remembered
/// one when their signatures match and it isn't clearly lower than where
/// that row was last seen (rows move up the feed, new ones enter at the
/// bottom; at most a row drops a little when one is slotted in above it). Rows are remembered until gone for `FORGET` seconds, so a
/// row is counted once however often detection loses and regains it (rows
/// flicker in and out, and live frames can be seconds apart) and however
/// far up the feed it moves.
///
/// The position check is what tells the local player's deaths apart:
/// their signatures all match (the victim side is always the player's own
/// name and portrait).
///
/// Times are seconds into the recording, so a recording can be re-analyzed
/// offline with the same results.
pub struct RowCounter {
    known: Vec<Known>,
    /// How much lower than last seen a row may appear and still be the
    /// same one, in row heights.
    lower: f32,
}

impl Default for RowCounter {
    /// For kills and assists, whose signatures name the victim: a row can
    /// drop a little when Valorant slots in a row above it.
    fn default() -> Self {
        Self { known: Vec::new(), lower: 1.5 }
    }
}

struct Known {
    sig: RowSig,
    /// Top scanline when last seen.
    y: usize,
    last_seen: f64,
}

/// A row stays up for about 5 s; detection can lose it for a few seconds
/// in between. One seen again after this long is new.
const FORGET: f64 = 6.0;
/// Signature tolerance, in row heights.
const SIG_TOLERANCE: f32 = 0.35;
impl RowCounter {
    /// For the local player's deaths, whose signatures all match: position
    /// is all that tells them apart, so only detection jitter is allowed.
    pub fn for_deaths() -> Self {
        Self { known: Vec::new(), lower: 0.5 }
    }

    /// Feeds the rows seen at time `t`; returns how many new events they
    /// represent.
    pub fn update(&mut self, rows: &SelfRows, t: f64) -> usize {
        self.known.retain(|k| t - k.last_seen < FORGET);
        let row_h = rows.row_h.max(1.0);
        let mut seen: Vec<(usize, RowSig)> = rows.rows.iter().map(|&(y0, _)| y0).zip(rows.sigs.iter().copied()).collect();
        // Top to bottom, each taking the lowest remembered row it can be:
        // the order of rows in the feed never changes.
        seen.sort_unstable();
        let mut taken = vec![false; self.known.len()];
        let mut new = 0;
        for (y, sig) in seen {
            let candidate = (0..self.known.len())
                .filter(|&i| {
                    let k = &self.known[i];
                    !taken[i]
                        && (k.sig as f32 - sig as f32).abs() <= SIG_TOLERANCE * row_h
                        && y as f32 <= k.y as f32 + self.lower * row_h
                })
                .min_by_key(|&i| self.known[i].y);
            match candidate {
                Some(i) => {
                    taken[i] = true;
                    let k = &mut self.known[i];
                    (k.sig, k.y, k.last_seen) = (sig, y, t);
                }
                None => {
                    new += 1;
                    self.known.push(Known { sig, y, last_seen: t });
                    taken.push(true);
                }
            }
        }
        new
    }
}
