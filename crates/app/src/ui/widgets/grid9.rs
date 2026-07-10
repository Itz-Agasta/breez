//! 3x3 anchor picker. Sets a normalized anchor point in {0, 0.5, 1}^2.

use eframe::egui::{CornerRadius, Rect, Sense, Ui, pos2, vec2};

use crate::theme;

const CELL: f32 = 18.0;
const GAP: f32 = 4.0;

pub fn grid9(ui: &mut Ui, anchor: &mut [f32; 2]) -> bool {
    let side = CELL * 3.0 + GAP * 2.0;
    let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    let mut changed = false;
    for row in 0..3 {
        for col in 0..3 {
            let min = pos2(
                rect.min.x + col as f32 * (CELL + GAP),
                rect.min.y + row as f32 * (CELL + GAP),
            );
            let cell = Rect::from_min_size(min, vec2(CELL, CELL));
            let response = ui.interact(cell, ui.id().with(("grid9", row, col)), Sense::click());
            let value = [col as f32 / 2.0, row as f32 / 2.0];
            let selected = *anchor == value;
            let bg = if selected {
                theme::BG_CONTROL_ACTIVE
            } else if response.hovered() {
                theme::BG_CONTROL_HOVER
            } else {
                theme::BG_CONTROL
            };
            ui.painter().rect_filled(cell, CornerRadius::same(4), bg);
            let dot = if selected {
                theme::ACCENT
            } else {
                theme::TEXT_LABEL
            };
            ui.painter().circle_filled(cell.center(), 2.0, dot);
            if response.clicked() && !selected {
                *anchor = value;
                changed = true;
            }
        }
    }
    changed
}
