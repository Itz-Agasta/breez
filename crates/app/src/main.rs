//! Breez entry point: frameless window setup and eframe bootstrap.

mod app;
mod export;
mod playback;
mod theme;
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    env_logger::init();
    // Every media path (record, thumbnails, waveforms, export) shells out to
    // ffmpeg. Resolving it up front turns a bare "No such file or directory"
    // at the first recording into one clear message, and is what makes the
    // README's "downloaded automatically on first run" true.
    log::info!("locating ffmpeg");
    if let Err(e) = breez_codec::ensure_ffmpeg() {
        eprintln!("Breez needs ffmpeg and could not obtain it: {e}");
        eprintln!("Install ffmpeg and put it on PATH, then start Breez again.");
        std::process::exit(1);
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Breez")
            .with_app_id("breez")
            .with_inner_size([1440.0, 884.0])
            .with_min_inner_size([1040.0, 640.0])
            .with_decorations(false),
        ..Default::default()
    };
    eframe::run_native(
        "Breez",
        options,
        Box::new(|cc| Ok(Box::new(app::BreezApp::new(cc)))),
    )
}
