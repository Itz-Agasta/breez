//! Background section: wallpaper swatches, padding/roundness/shadow sliders,
//! aspect ratio.

use eframe::egui::Ui;

use super::{row_label, slider_row};
use crate::app::Session;
use crate::theme;
use crate::ui::widgets::{self, segmented::Segment};

const RATIOS: &[&str] = &["16:9", "9:16", "1:1"];

pub fn show(ui: &mut Ui, session: &mut Session) {
    let style = &mut session.project.style;

    row_label(ui, "Wallpaper");
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        for (id, top, bottom) in theme::WALLPAPERS {
            let selected = style.wallpaper == *id;
            if widgets::swatch::swatch(ui, *top, *bottom, selected)
                .on_hover_text(*id)
                .clicked()
            {
                style.wallpaper = (*id).to_owned();
            }
        }
    });
    ui.add_space(14.0);

    let mut padding = style.padding as f32;
    if slider_row(ui, "Padding", &mut padding, 16.0..=140.0, |v| {
        format!("{}", v.round() as u32)
    }) {
        style.padding = padding.round() as u32;
    }
    let mut radius = style.radius as f32;
    if slider_row(ui, "Roundness", &mut radius, 0.0..=28.0, |v| {
        format!("{}", v.round() as u32)
    }) {
        style.radius = radius.round() as u32;
    }
    let mut shadow = style.shadow as f32;
    if slider_row(ui, "Shadow", &mut shadow, 0.0..=100.0, |v| {
        format!("{}", v.round() as u32)
    }) {
        style.shadow = shadow.round() as u32;
    }

    row_label(ui, "Ratio");
    ui.add_space(6.0);
    let mut ratio_idx = RATIOS.iter().position(|r| *r == style.ratio).unwrap_or(0);
    let segments: Vec<Segment> = RATIOS.iter().map(|r| Segment::new(r)).collect();
    if widgets::segmented::segmented(ui, &mut ratio_idx, &segments, 28.0) {
        style.ratio = RATIOS[ratio_idx].to_owned();
    }
}
