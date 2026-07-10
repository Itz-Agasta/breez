//! Zoom & pan section. Static in Phase 2: controls edit a draft segment that
//! real zoom keyframes bind to in Phase 4.

use eframe::egui::{Align, Layout, Ui};

use super::{row_label, slider_row, switch_row};
use breez_core::project::{Easing, ZoomSegment};

use crate::theme;
use crate::ui::widgets::{self, segmented::Segment};

pub fn show(ui: &mut Ui, draft: &mut ZoomSegment) {
    ui.horizontal(|ui| {
        widgets::chip::chip(ui, "0 keyframes", theme::TEXT_MUTED);
    });
    ui.add_space(12.0);

    slider_row(ui, "Level", &mut draft.level, 1.0..=3.0, |v| {
        format!("{v:.1}x")
    });

    row_label(ui, "Easing");
    ui.add_space(6.0);
    let mut easing_idx = match draft.easing {
        Easing::Linear => 0,
        Easing::Smooth => 1,
        Easing::Snap => 2,
    };
    if widgets::segmented::segmented(
        ui,
        &mut easing_idx,
        &[
            Segment::new("Linear"),
            Segment::new("Smooth"),
            Segment::new("Snap"),
        ],
        28.0,
    ) {
        draft.easing = [Easing::Linear, Easing::Smooth, Easing::Snap][easing_idx];
    }
    ui.add_space(12.0);

    switch_row(ui, "Follow cursor", &mut draft.follow_cursor);

    ui.horizontal(|ui| {
        row_label(ui, "Anchor");
        ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
            widgets::grid9::grid9(ui, &mut draft.anchor);
        });
    });
}
