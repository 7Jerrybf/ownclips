// Runs the killfeed detector over a recording, for checking it against what
// actually happened. Reads raw RGB frames of the killfeed crop from stdin
// (see the ffmpeg command below) and prints one CSV line per frame, or with
// a PNG path instead of the numbers, the rows found in that one image.
//
//   ffmpeg -i match.mp4 -vf "fps=10,crop=W:H:X:Y" -f rawvideo -pix_fmt rgb24 - |
//     cargo run --release --example replay_killfeed -- W H FPS GAME_HEIGHT
use std::io::{BufWriter, Read, Write};

use ownclips::killfeed;

fn main() {
    if let Some(png) = std::env::args().nth(1).filter(|a| a.ends_with(".png")) {
        let game_h: f32 = std::env::args().nth(2).and_then(|a| a.parse().ok()).unwrap_or(1440.0);
        let s = killfeed::scan(&image::open(png).unwrap().into_rgb8(), killfeed::ROW_HEIGHT_FRAC * game_h);
        println!("kills {:?}
deaths {:?}
assists {:?}", s.kills.rows, s.deaths.rows, s.assists.rows);
        return;
    }
    let args: Vec<f32> = std::env::args().skip(1).map(|a| a.parse().expect("number")).collect();
    let [w, h, fps, game_h] = args[..] else { panic!("usage: W H FPS GAME_HEIGHT") };
    let (w, h) = (w as u32, h as u32);
    let row_h = killfeed::ROW_HEIGHT_FRAC * game_h;
    let mut buf = vec![0u8; (w * h * 3) as usize];
    let mut stdin = std::io::stdin().lock();
    let mut out = BufWriter::new(std::io::stdout().lock());
    writeln!(out, "t,kills,kills_newest,deaths,deaths_newest,assists,assists_newest").unwrap();
    let mut i = 0u64;
    while stdin.read_exact(&mut buf).is_ok() {
        let img = image::RgbImage::from_raw(w, h, buf.clone()).unwrap();
        let s = killfeed::scan(&img, row_h);
        writeln!(
            out,
            "{:.2},{},{},{},{},{},{}",
            i as f32 / fps,
            s.kills.count,
            s.kills.newest as u8,
            s.deaths.count,
            s.deaths.newest as u8,
            s.assists.count,
            s.assists.newest as u8
        )
        .unwrap();
        i += 1;
    }
}
