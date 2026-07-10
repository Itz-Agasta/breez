//! 302px slide-out tool panel. Phase 2 ships stub content for Media and
//! Music; imports arrive with playback (Phase 3) and music with Phase 5.

use eframe::egui::{
    CornerRadius, FontFamily, FontId, Frame, Panel, RichText, Stroke, StrokeKind, Ui, vec2,
};

use super::Tool;
use crate::theme;

pub fn show(ui: &mut Ui, tool: Tool) {
    Panel::left("toolpanel")
        .exact_size(theme::TOOLPANEL_WIDTH)
        .frame(Frame::new().fill(theme::BG_PANEL).inner_margin(16))
        .show_separator_line(false)
        .show(ui, |ui| {
            let panel = ui.max_rect();
            ui.painter().vline(
                panel.max.x + 15.5,
                panel.y_range().expand(16.0),
                Stroke::new(1.0, theme::BORDER),
            );
            ui.label(
                RichText::new(tool.title())
                    .font(FontId::new(13.0, theme::semibold()))
                    .color(theme::TEXT),
            );
            ui.add_space(14.0);
            match tool {
                Tool::Media => {
                    muted(ui, "No imports yet.");
                    ui.add_space(6.0);
                    faint(
                        ui,
                        "Recorded takes appear here; file import lands in Phase 3.",
                    );
                }
                Tool::Music => {
                    muted(ui, "No music tracks.");
                    ui.add_space(10.0);
                    drop_stub(ui);
                }
            }
        });
}

fn muted(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(FontId::new(12.5, FontFamily::Proportional))
            .color(theme::TEXT_MUTED),
    );
}

fn faint(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(FontId::new(11.5, FontFamily::Proportional))
            .color(theme::TEXT_FAINT),
    );
}

fn drop_stub(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), 84.0),
        eframe::egui::Sense::hover(),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::RADIUS_CARD),
        Stroke::new(1.0, theme::BORDER_STRONG),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        eframe::egui::Align2::CENTER_CENTER,
        "Audio import lands in Phase 5",
        FontId::new(11.5, FontFamily::Proportional),
        theme::TEXT_FAINT,
    );
}
