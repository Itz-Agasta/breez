//! Transport bar: prev/play/next, current time / duration, edit tool icons.
//! Everything except the time readout is disabled until playback (Phase 3).

use eframe::egui::{
    Align, Align2, Color32, FontFamily, FontId, Layout, Sense, Ui, UiBuilder, vec2,
};

use crate::theme;
use crate::ui::editor::format_ns;
use crate::ui::widgets;

pub fn show(ui: &mut Ui, duration_ns: u64) {
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme::TRANSPORT_HEIGHT),
        Sense::hover(),
    );
    let mut bar = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 0.0))));
    bar.horizontal_centered(|ui| {
        widgets::button::icon(ui, "\u{23ee}", false).on_hover_text("Playback lands in Phase 3");
        play_button(ui);
        widgets::button::icon(ui, "\u{23ed}", false).on_hover_text("Playback lands in Phase 3");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            widgets::button::ghost(ui, "Fit", false)
                .on_hover_text("Timeline zoom lands in Phase 3");
            ui.add_space(4.0);
            widgets::button::icon(ui, "\u{1f50a}", false).on_hover_text("Audio lands in Phase 3");
            widgets::button::icon(ui, "\u{25c6}", false)
                .on_hover_text("Zoom keyframes land in Phase 4");
            widgets::button::icon(ui, "\u{2702}", false).on_hover_text("Split lands in Phase 3");
        });
    });
    // Centered time readout painted over the bar so button layout can't shift it.
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        format!("0:00 / {}", format_ns(duration_ns)),
        FontId::new(12.5, FontFamily::Monospace),
        theme::TEXT_MUTED,
    );
}

fn play_button(ui: &mut Ui) {
    let (rect, response) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 17.0, theme::BG_CONTROL_ACTIVE);
    ui.painter().text(
        rect.center() + vec2(1.0, 0.0),
        Align2::CENTER_CENTER,
        "\u{25b6}",
        FontId::proportional(13.0),
        Color32::from_rgb(0x6a, 0x6a, 0x6a),
    );
    response.on_hover_text("Playback lands in Phase 3");
}
