//! Segmented control: a rounded container with one active segment.

use eframe::egui::{Align2, Color32, CornerRadius, FontId, Rect, Sense, Ui, pos2, vec2};

use crate::theme;

pub struct Segment<'a> {
    pub label: &'a str,
    pub enabled: bool,
}

impl<'a> Segment<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            enabled: true,
        }
    }

    pub fn disabled(label: &'a str) -> Self {
        Self {
            label,
            enabled: false,
        }
    }
}

/// Draws the control, mutates `selected` on click. Returns true on change.
pub fn segmented(ui: &mut Ui, selected: &mut usize, segments: &[Segment<'_>], height: f32) -> bool {
    let font = FontId::new(12.0, theme::medium());
    let pad = 14.0;
    let widths: Vec<f32> = segments
        .iter()
        .map(|s| {
            ui.painter()
                .layout_no_wrap(s.label.to_owned(), font.clone(), Color32::PLACEHOLDER)
                .size()
                .x
                + pad * 2.0
        })
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + 6.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, height), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::RADIUS_BUTTON),
        theme::BG_CONTROL,
    );

    let mut changed = false;
    let mut x = rect.min.x + 3.0;
    for (i, (segment, width)) in segments.iter().zip(&widths).enumerate() {
        let seg_rect =
            Rect::from_min_max(pos2(x, rect.min.y + 3.0), pos2(x + width, rect.max.y - 3.0));
        x += width;
        let response = ui.interact(
            seg_rect,
            ui.id().with(("segment", i)),
            if segment.enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let active = i == *selected;
        if active {
            ui.painter().rect_filled(
                seg_rect,
                CornerRadius::same(theme::RADIUS_BUTTON - 2),
                theme::BG_CONTROL_ACTIVE,
            );
        } else if segment.enabled && response.hovered() {
            ui.painter().rect_filled(
                seg_rect,
                CornerRadius::same(theme::RADIUS_BUTTON - 2),
                theme::BG_CONTROL_HOVER,
            );
        }
        let fg = if !segment.enabled {
            theme::TEXT_LABEL
        } else if active {
            theme::TEXT
        } else {
            theme::TEXT_MUTED
        };
        ui.painter().text(
            seg_rect.center(),
            Align2::CENTER_CENTER,
            segment.label,
            font.clone(),
            fg,
        );
        if segment.enabled && response.clicked() && !active {
            *selected = i;
            changed = true;
        }
    }
    changed
}
