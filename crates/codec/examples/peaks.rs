//! Headless peaks check: decode an audio file into waveform peaks.
//!
//! Usage: cargo run -p breez-codec --example peaks -- <audio> [cache.json]

use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: peaks <audio> [cache.json]");
    let cache = args
        .next()
        .unwrap_or_else(|| "/tmp/breez-peaks.json".to_owned());

    breez_codec::ensure_ffmpeg().expect("ffmpeg unavailable");

    let t0 = Instant::now();
    let peaks =
        breez_codec::generate_peaks(std::path::Path::new(&path), std::path::Path::new(&cache))
            .expect("peaks");
    let max = peaks.peaks.iter().copied().fold(0f32, f32::max);
    println!(
        "{} buckets ({}per s), duration {:.3}s, max amplitude {max:.3} in {:?}",
        peaks.peaks.len(),
        peaks.peaks_per_sec,
        peaks.duration_ns as f64 / 1e9,
        t0.elapsed()
    );
}
