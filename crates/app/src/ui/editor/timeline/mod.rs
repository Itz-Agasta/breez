//! Timeline chrome: transport bar, ruler, and empty lanes. Real clips,
//! playhead, and waveforms arrive in Phases 3-5.

mod lanes;
mod ruler;
mod transport;

use eframe::egui::{Frame, Panel, Stroke, Ui};

use crate::app::Session;
use crate::theme;

pub fn show(ui: &mut Ui, session: &Session) {
    Panel::bottom("timeline")
        .exact_size(theme::TIMELINE_HEIGHT)
        .frame(Frame::new().fill(theme::BG_PANEL))
        .show_separator_line(false)
        .show(ui, |ui| {
            let panel = ui.max_rect();
            ui.painter().hline(
                panel.x_range(),
                panel.min.y + 0.5,
                Stroke::new(1.0, theme::BORDER),
            );
            let duration_ns = timeline_duration_ns(session);
            transport::show(ui, duration_ns);
            ruler::show(ui, duration_ns);
            lanes::show(ui);
        });
}

fn timeline_duration_ns(session: &Session) -> u64 {
    session
        .project
        .timeline
        .clips
        .iter()
        .map(|c| c.src_out_ns.saturating_sub(c.src_in_ns))
        .sum()
}
