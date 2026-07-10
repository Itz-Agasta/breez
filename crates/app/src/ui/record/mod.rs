//! Record view: desk gradient backdrop, hint pill, and the floating pill bar
//! with source segments, timer, and the record button.
//!
//! The view is stateless: the app hands in a [`RecordState`] snapshot and
//! receives an optional [`RecordAction`] back. On Wayland the portal picks
//! the display, so the Window segment is hidden and Area waits on X11.

use std::time::Duration;

use eframe::egui::{
    Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Rect, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, pos2, vec2,
};

use crate::theme;
use crate::ui::widgets::{self, segmented::Segment};

pub enum RecordState {
    Idle,
    Recording { elapsed: Duration },
    Saving,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordAction {
    Start,
    Stop,
}

pub fn show(ui: &mut Ui, state: &RecordState, error: Option<&str>) -> Option<RecordAction> {
    let mut action = None;
    eframe::egui::CentralPanel::no_frame()
        .frame(Frame::new())
        .show(ui, |ui| {
            let full = ui.max_rect();
            widgets::vertical_gradient(
                ui.painter(),
                full,
                0,
                Color32::from_rgb(0x10, 0x10, 0x14),
                theme::BG_PANEL_DARK,
            );
            center_placeholder(ui, full, state);
            if matches!(state, RecordState::Idle) {
                hint_pill(ui, full);
            }
            if let Some(error) = error {
                error_line(ui, full, error);
            }
            action = pill_bar(ui, full, state);
        });
    action
}

fn center_placeholder(ui: &mut Ui, full: Rect, state: &RecordState) {
    let (title, subtitle) = match state {
        RecordState::Idle => (
            "Ready to record",
            "The screen you pick shows up in the editor after you stop.",
        ),
        RecordState::Recording { .. } => ("Recording", "Capturing display and system audio."),
        RecordState::Saving => (
            "Saving take",
            "Finishing the encoders and writing the package.",
        ),
    };
    ui.painter().text(
        full.center() - vec2(0.0, 14.0),
        Align2::CENTER_CENTER,
        title,
        FontId::new(17.0, theme::semibold()),
        theme::TEXT,
    );
    ui.painter().text(
        full.center() + vec2(0.0, 12.0),
        Align2::CENTER_CENTER,
        subtitle,
        FontId::new(12.5, FontFamily::Proportional),
        theme::TEXT_MUTED,
    );
}

fn hint_pill(ui: &mut Ui, full: Rect) {
    let text = if is_wayland() {
        "The system portal picks the display when recording starts"
    } else {
        "Full screen capture \u{00b7} window and area selection coming soon"
    };
    let font = FontId::new(11.5, FontFamily::Proportional);
    let width = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), Color32::PLACEHOLDER)
        .size()
        .x;
    let rect = Rect::from_center_size(
        pos2(full.center().x, full.min.y + 34.0),
        vec2(width + 28.0, 28.0),
    );
    ui.painter()
        .rect_filled(rect, CornerRadius::same(14), theme::BG_CONTROL);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(14),
        Stroke::new(1.0, theme::BORDER),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        font,
        theme::TEXT_MUTED,
    );
}

fn error_line(ui: &mut Ui, full: Rect, error: &str) {
    ui.painter().text(
        pos2(full.center().x, full.max.y - 116.0),
        Align2::CENTER_CENTER,
        error,
        FontId::new(11.5, FontFamily::Proportional),
        theme::RECORD_RED,
    );
}

fn pill_bar(ui: &mut Ui, full: Rect, state: &RecordState) -> Option<RecordAction> {
    let bar_height = 68.0;
    let bar_width = 470.0;
    let bar_rect = Rect::from_center_size(
        pos2(full.center().x, full.max.y - 24.0 - bar_height / 2.0),
        vec2(bar_width, bar_height),
    );
    let radius = CornerRadius::same((bar_height / 2.0) as u8);
    ui.painter().rect_filled(
        bar_rect,
        radius,
        Color32::from_rgba_unmultiplied(0x14, 0x14, 0x14, 0xf2),
    );
    ui.painter().rect_stroke(
        bar_rect,
        radius,
        Stroke::new(1.0, theme::BORDER_STRONG),
        StrokeKind::Inside,
    );

    let mut action = None;
    let mut bar_ui = ui.new_child(UiBuilder::new().max_rect(bar_rect.shrink2(vec2(20.0, 0.0))));
    bar_ui.horizontal_centered(|ui| {
        source_segments(ui);
        separator(ui);
        widgets::button::icon(ui, "\u{1f4f7}", false).on_hover_text("Camera arrives post-MVP");
        widgets::button::icon(ui, "\u{1f3a4}", false)
            .on_hover_text("Mic arrives post-MVP (pinray backend)");
        separator(ui);
        timer_label(ui, state);
        ui.add_space(6.0);
        action = record_button(ui, state);
        ui.add_space(2.0);
        widgets::button::icon(ui, "\u{2699}", false).on_hover_text("Settings land in Phase 7");
    });
    action
}

fn source_segments(ui: &mut Ui) {
    let mut selected = 0;
    let segments = if is_wayland() {
        vec![Segment::new("Screen"), Segment::disabled("Area")]
    } else {
        vec![
            Segment::new("Screen"),
            Segment::disabled("Window"),
            Segment::disabled("Area"),
        ]
    };
    widgets::segmented::segmented(ui, &mut selected, &segments, 32.0);
}

fn separator(ui: &mut Ui) {
    ui.add_space(10.0);
    let rect = ui.max_rect();
    let x = ui.cursor().min.x;
    ui.painter().vline(
        x,
        eframe::egui::Rangef::new(rect.center().y - 12.0, rect.center().y + 12.0),
        Stroke::new(1.0, theme::BORDER_STRONG),
    );
    ui.add_space(11.0);
}

fn timer_label(ui: &mut Ui, state: &RecordState) {
    let (text, color) = match state {
        RecordState::Idle => ("0:00".to_owned(), theme::TEXT_MUTED),
        RecordState::Recording { elapsed } => (format_secs(elapsed.as_secs()), theme::TEXT),
        RecordState::Saving => ("Saving\u{2026}".to_owned(), theme::TEXT_MUTED),
    };
    let (rect, _) = ui.allocate_exact_size(vec2(52.0, 20.0), Sense::hover());
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::new(13.0, FontFamily::Monospace),
        color,
    );
}

/// 52px record button: white circle, red dot when idle, red square while
/// recording. Disabled while the take is being saved.
fn record_button(ui: &mut Ui, state: &RecordState) -> Option<RecordAction> {
    let saving = matches!(state, RecordState::Saving);
    let (rect, response) = ui.allocate_exact_size(
        vec2(52.0, 52.0),
        if saving {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
    let bg = if saving {
        theme::BG_CONTROL_ACTIVE
    } else if response.hovered() {
        Color32::from_rgb(0xd9, 0xd9, 0xd9)
    } else {
        Color32::WHITE
    };
    ui.painter().circle_filled(rect.center(), 26.0, bg);
    match state {
        RecordState::Recording { .. } => {
            ui.painter().rect_filled(
                Rect::from_center_size(rect.center(), vec2(18.0, 18.0)),
                CornerRadius::same(4),
                theme::RECORD_RED,
            );
        }
        _ => {
            ui.painter()
                .circle_filled(rect.center(), 9.0, theme::RECORD_RED);
        }
    }
    if response.clicked() {
        return match state {
            RecordState::Idle => Some(RecordAction::Start),
            RecordState::Recording { .. } => Some(RecordAction::Stop),
            RecordState::Saving => None,
        };
    }
    None
}

pub fn format_secs(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}
