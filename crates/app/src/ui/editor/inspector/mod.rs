//! Right inspector: collapsible sections of styled rows. Phase 2 renders the
//! sections statically; values bind to the loaded project style but nothing
//! is persisted or previewed live until Phase 3.

mod audio;
mod background;
mod cursor;
mod zoom;

use eframe::egui::{
    Align, Align2, FontFamily, FontId, Frame, Layout, Panel, RichText, ScrollArea, Sense, Stroke,
    Ui, vec2,
};
use std::ops::RangeInclusive;

use super::EditorState;
use crate::app::Session;
use crate::theme;
use crate::ui::widgets;

pub fn show(ui: &mut Ui, state: &mut EditorState, session: &mut Session) {
    Panel::right("inspector")
        .exact_size(theme::INSPECTOR_WIDTH)
        .frame(Frame::new().fill(theme::BG_PANEL))
        .show_separator_line(false)
        .show(ui, |ui| {
            let panel = ui.max_rect();
            ui.painter().vline(
                panel.min.x + 0.5,
                panel.y_range(),
                Stroke::new(1.0, theme::BORDER),
            );
            ScrollArea::vertical().show(ui, |ui| {
                let mut background_open = state.background_open;
                section(ui, "Background", &mut background_open, |ui| {
                    background::show(ui, session);
                });
                state.background_open = background_open;

                let mut zoom_open = state.zoom_open;
                section(ui, "Zoom & pan", &mut zoom_open, |ui| {
                    zoom::show(ui, &mut state.zoom_draft);
                });
                state.zoom_open = zoom_open;

                let mut cursor_open = state.cursor_open;
                section(ui, "Cursor", &mut cursor_open, |ui| {
                    cursor::show(ui, session);
                });
                state.cursor_open = cursor_open;

                let mut audio_open = state.audio_open;
                section(ui, "Audio", &mut audio_open, |ui| {
                    audio::show(ui, session);
                });
                state.audio_open = audio_open;
            });
        });
}

fn section(ui: &mut Ui, title: &str, open: &mut bool, body: impl FnOnce(&mut Ui)) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    if response.clicked() {
        *open = !*open;
    }
    let chevron = if *open { "\u{25be}" } else { "\u{25b8}" };
    ui.painter().text(
        rect.left_center() + vec2(16.0, 0.0),
        Align2::LEFT_CENTER,
        chevron,
        FontId::proportional(11.0),
        theme::TEXT_LABEL,
    );
    ui.painter().text(
        rect.left_center() + vec2(32.0, 0.0),
        Align2::LEFT_CENTER,
        title,
        FontId::new(12.5, theme::medium()),
        if *open {
            theme::TEXT
        } else {
            theme::TEXT_MUTED
        },
    );
    if *open {
        Frame::new()
            .inner_margin(eframe::egui::Margin {
                left: 16,
                right: 16,
                top: 2,
                bottom: 14,
            })
            .show(ui, body);
    }
    let line_y = ui.cursor().min.y;
    ui.painter()
        .hline(rect.x_range(), line_y, Stroke::new(1.0, theme::BORDER));
    ui.add_space(1.0);
}

/// Label left, formatted value right, slider underneath. Returns true when
/// the value changed this frame.
pub(super) fn slider_row(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: RangeInclusive<f32>,
    format: impl Fn(f32) -> String,
) -> bool {
    row_header(ui, label, &format(*value));
    let changed = widgets::slider::slider(ui, value, range).changed();
    ui.add_space(10.0);
    changed
}

/// Label left, switch right on a single row.
pub(super) fn switch_row(ui: &mut Ui, label: &str, on: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        row_label(ui, label);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed = widgets::switch::switch(ui, on).changed();
        });
    });
    ui.add_space(10.0);
    changed
}

pub(super) fn row_header(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        row_label(ui, label);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .font(FontId::new(11.5, FontFamily::Monospace))
                    .color(theme::TEXT_MUTED),
            );
        });
    });
}

pub(super) fn row_label(ui: &mut Ui, label: &str) {
    ui.label(
        RichText::new(label)
            .font(FontId::new(12.0, FontFamily::Proportional))
            .color(theme::TEXT_MUTED),
    );
}
