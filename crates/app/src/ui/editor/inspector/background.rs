//! Background section: wallpaper swatches, padding/roundness/shadow sliders,
//! aspect ratio.

use eframe::egui::Ui;

use super::{row_label, slider_row};
use crate::app::Session;
use crate::theme;
use crate::ui::widgets::{self, segmented::Segment};
use breez_core::project::{PADDING_RANGE, RADIUS_RANGE, RATIOS, SHADOW_RANGE};

/// Returns true when any style value changed this frame.
pub fn show(ui: &mut Ui, session: &mut Session) -> bool {
    let style = &mut session.project.style;
    let mut changed = false;

    row_label(ui, "Wallpaper");
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        for (id, top, bottom) in theme::wallpapers() {
            let selected = style.wallpaper == id;
            if widgets::swatch::swatch(ui, top, bottom, selected)
                .on_hover_text(id)
                .clicked()
            {
                style.wallpaper = id.to_owned();
                changed = true;
            }
        }
    });
    ui.add_space(14.0);

    changed |= u32_slider_row(ui, "Padding", &mut style.padding, PADDING_RANGE);
    changed |= u32_slider_row(ui, "Roundness", &mut style.radius, RADIUS_RANGE);
    changed |= u32_slider_row(ui, "Shadow", &mut style.shadow, SHADOW_RANGE);

    row_label(ui, "Ratio");
    ui.add_space(6.0);
    let mut ratio_idx = RATIOS.iter().position(|r| *r == style.ratio).unwrap_or(0);
    let segments: Vec<Segment> = RATIOS.iter().map(|r| Segment::new(r)).collect();
    if widgets::segmented::segmented(ui, &mut ratio_idx, &segments, 28.0) {
        style.ratio = RATIOS[ratio_idx].to_owned();
        changed = true;
    }
    changed
}

fn u32_slider_row(
    ui: &mut Ui,
    label: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
) -> bool {
    let mut v = *value as f32;
    let changed = slider_row(
        ui,
        label,
        &mut v,
        *range.start() as f32..=*range.end() as f32,
        |v| format!("{}", v.round() as u32),
    );
    if changed {
        *value = v.round() as u32;
    }
    changed
}
