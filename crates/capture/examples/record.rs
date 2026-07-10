//! Headless recording smoke test:
//! `cargo run -p breez-capture --example record -- <seconds> <out.rec>`

use std::time::Duration;

use breez_capture::{RecordConfig, Recorder};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let mut args = std::env::args().skip(1);
    let seconds: u64 = args
        .next()
        .and_then(|s| s.parse().ok())
        .expect("usage: record <seconds> <out.rec>");
    let out = args.next().expect("usage: record <seconds> <out.rec>");

    breez_codec::ensure_ffmpeg().expect("ffmpeg unavailable");

    println!("recording {seconds}s to {out} ...");
    let recorder = Recorder::start(&out, RecordConfig::default()).expect("start failed");
    std::thread::sleep(Duration::from_secs(seconds));
    let summary = recorder.stop().expect("recording failed");
    println!(
        "done: take {} - {}x{}@{} - {:.1}s, {} frames, {} audio packets",
        summary.take_id,
        summary.width,
        summary.height,
        summary.fps,
        summary.duration_ns as f64 / 1e9,
        summary.video_frames,
        summary.audio_packets,
    );
}
