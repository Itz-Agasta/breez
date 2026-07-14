//! Playhead: accent vertical line with a triangle head at the top of the
//! lane region. Painted last so it overlays ruler and lanes; seeking happens
//! on the ruler/lane interactions, the playhead itself is passive.

use eframe::egui::{Pos2, Rect, Shape, Stroke, Ui, pos2};

use super::Track;
use crate::theme;

pub fn show(ui: &Ui, region: Rect, track: &Track, playhead_ns: u64) {
    if track.duration_ns == 0 {
        return;
    }
    let x = track.x_at(playhead_ns);
    ui.painter()
        .vline(x, region.y_range(), Stroke::new(1.5, theme::ACCENT));
    let head: Vec<Pos2> = vec![
        pos2(x - 5.0, region.min.y),
        pos2(x + 5.0, region.min.y),
        pos2(x, region.min.y + 7.0),
    ];
    ui.painter()
        .add(Shape::convex_polygon(head, theme::ACCENT, Stroke::NONE));
}
