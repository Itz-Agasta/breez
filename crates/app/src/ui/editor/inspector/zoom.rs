//! Zoom & pan section: keyframe count chip plus the controls for the zoom
//! segment selected in the timeline's Zoom lane. With no selection it only
//! hints at how to add one.

use eframe::egui::{Align, FontFamily, FontId, Layout, RichText, Ui};

use super::{row_label, slider_row, switch_row};
use crate::app::Session;
use crate::theme;
use crate::ui::editor::EditorState;
use crate::ui::widgets::{self, segmented::Segment};
use breez_core::project::{Easing, ZOOM_LEVEL_RANGE};

/// Returns true when a value changed this frame.
pub fn show(ui: &mut Ui, state: &mut EditorState, session: &mut Session) -> bool {
    let count = session.project.timeline.zoom.len();
    ui.horizontal(|ui| {
        let plural = if count == 1 { "" } else { "s" };
        widgets::chip::chip(ui, &format!("{count} keyframe{plural}"), theme::TEXT_MUTED);
    });
    ui.add_space(12.0);

    let Some(segment) = state
        .selected_zoom
        .and_then(|index| session.project.timeline.zoom.get_mut(index))
    else {
        ui.label(
            RichText::new("Double-click the Zoom lane to add a keyframe, then select it here.")
                .font(FontId::new(11.0, FontFamily::Proportional))
                .color(theme::TEXT_FAINT),
        );
        return false;
    };

    let mut changed = slider_row(ui, "Level", &mut segment.level, ZOOM_LEVEL_RANGE, |v| {
        format!("{v:.1}x")
    });

    row_label(ui, "Easing");
    ui.add_space(6.0);
    let mut easing_idx = match segment.easing {
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
        segment.easing = [Easing::Linear, Easing::Smooth, Easing::Snap][easing_idx];
        changed = true;
    }
    ui.add_space(12.0);

    changed |= switch_row(ui, "Follow cursor", &mut segment.follow_cursor);

    ui.horizontal(|ui| {
        row_label(ui, "Anchor");
        ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
            changed |= widgets::grid9::grid9(ui, &mut segment.anchor);
        });
    });
    changed
}
