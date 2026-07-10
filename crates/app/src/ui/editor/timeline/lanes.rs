//! Lane rows with gutter labels. Bodies are empty chrome in Phase 2; clip,
//! keyframe, cursor, and waveform painters land in Phases 3-5.

use eframe::egui::{Align2, FontFamily, FontId, Sense, Stroke, Ui, pos2, vec2};

use crate::theme;

const LANES: &[(&str, f32)] = &[
    ("Screen", 54.0),
    ("Zoom", 30.0),
    ("Cursor", 22.0),
    ("Music", 34.0),
];

pub fn show(ui: &mut Ui) {
    // Rows must touch so gutter and separator lines stay continuous.
    ui.spacing_mut().item_spacing.y = 0.0;
    for (label, height) in LANES {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), *height), Sense::hover());
        ui.painter().text(
            pos2(rect.min.x + 14.0, rect.center().y),
            Align2::LEFT_CENTER,
            *label,
            FontId::new(11.0, FontFamily::Proportional),
            theme::TEXT_LABEL,
        );
        ui.painter().vline(
            rect.min.x + theme::GUTTER_WIDTH - 8.0,
            rect.y_range(),
            Stroke::new(1.0, theme::BORDER),
        );
        ui.painter().hline(
            rect.x_range(),
            rect.max.y - 0.5,
            Stroke::new(1.0, theme::BORDER),
        );
    }
}
