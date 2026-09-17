//! Breez entry point: frameless window setup and eframe bootstrap.

mod app;
mod export;
mod playback;
mod theme;
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    env_logger::init();
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
