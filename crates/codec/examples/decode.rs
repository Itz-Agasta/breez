//! Headless decode check: seek into a video, pull frames, time the seek.
//!
//! Usage: cargo run -p breez-codec --example decode -- <video> [seek_secs]

use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: decode <video> [seek_secs]");
    let seek_secs: f64 = args.next().map_or(2.0, |s| s.parse().expect("seek_secs"));

    breez_codec::ensure_ffmpeg().expect("ffmpeg unavailable");
    let start_ns = (seek_secs * 1e9) as u64;

    let t0 = Instant::now();
    let mut decoder =
        breez_codec::VideoDecoder::open(std::path::Path::new(&path), start_ns).expect("open");
    let first = decoder.next_frame().expect("decode").expect("no frame");
    println!(
        "seek {seek_secs}s -> first frame {}x{} pts {:.3}s in {:?}",
        first.width,
        first.height,
        first.pts_ns as f64 / 1e9,
        t0.elapsed()
    );

    let t1 = Instant::now();
    let mut count = 1u32;
    while count < 60 {
        match decoder.next_frame().expect("decode") {
            Some(_) => count += 1,
            None => break,
        }
    }
    println!(
        "{count} frames total, next {} in {:?}",
        count - 1,
        t1.elapsed()
    );
}
