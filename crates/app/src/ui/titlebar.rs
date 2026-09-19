//! Custom titlebar for the frameless window: logo, project breadcrumb, the
//! Record/Edit mode toggle, Share/Export actions, drag region, and
//! min/max/close controls.

use eframe::egui::{
    Align, Align2, Color32, Context, CornerRadius, FontFamily, FontId, Frame, Id, Layout, Panel,
    PointerButton, Rect, RichText, Sense, Stroke, Ui, UiBuilder, Vec2, ViewportCommand, vec2,
};

use crate::app::Mode;
use crate::theme;
use crate::ui::widgets::{self, segmented::Segment};

pub struct TitlebarState<'a> {
    pub mode: Mode,
    pub project_name: Option<&'a str>,
    pub can_edit: bool,
    /// Recording or saving: mode switching is blocked.
    pub busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitlebarAction {
    SetMode(Mode),
    Export,
}

pub fn show(root: &mut Ui, state: &TitlebarState<'_>) -> Option<TitlebarAction> {
    let mut action = None;
    Panel::top("titlebar")
        .exact_size(theme::TITLEBAR_HEIGHT)
        .frame(Frame::new().fill(theme::BG_TITLEBAR))
        .show_separator_line(false)
        .show(root, |ui| {
            let bar_rect = ui.max_rect();
            drag_region(ui, bar_rect);
            left_section(ui, bar_rect, state);
            action = mode_toggle(ui, bar_rect, state);
            action = right_section(ui, bar_rect, state).or(action);
            ui.painter().hline(
                bar_rect.x_range(),
                bar_rect.bottom(),
                Stroke::new(1.0, theme::BORDER),
            );
        });
    action
}

/// Whole bar drags the window; double-click toggles maximize. Runs before the
/// buttons so their interactions take precedence in egui's hit order.
fn drag_region(ui: &mut Ui, rect: Rect) {
    let response = ui.interact(rect, Id::new("titlebar-drag"), Sense::click_and_drag());
    if response.drag_started_by(PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if response.double_clicked_by(PointerButton::Primary) {
        toggle_maximize(ui.ctx());
    }
}

fn left_section(ui: &mut Ui, bar_rect: Rect, state: &TitlebarState<'_>) {
    let mut ui = ui.new_child(UiBuilder::new().max_rect(bar_rect.shrink2(vec2(12.0, 0.0))));
    ui.horizontal_centered(|ui| {
        logo(ui);
        ui.add_space(10.0);
        ui.label(
            RichText::new(state.project_name.unwrap_or("Untitled"))
                .font(FontId::new(13.0, theme::medium()))
                .color(theme::TEXT),
        );
        if state.can_edit {
            ui.add_space(8.0);
            widgets::chip::chip(ui, "Draft", theme::TEXT_MUTED);
        }
    });
}

fn logo(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(6), theme::TEXT);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        "B",
        FontId::new(12.0, FontFamily::Name("geist-bold".into())),
        Color32::BLACK,
    );
}

/// Centered Record/Edit segmented toggle.
fn mode_toggle(ui: &mut Ui, bar_rect: Rect, state: &TitlebarState<'_>) -> Option<TitlebarAction> {
    let toggle_rect = Rect::from_center_size(bar_rect.center(), vec2(160.0, bar_rect.height()));
    let mut ui = ui.new_child(UiBuilder::new().max_rect(toggle_rect));
    let mut action = None;
    ui.horizontal_centered(|ui| {
        let mut selected = match state.mode {
            Mode::Record => 0,
            Mode::Edit => 1,
        };
        let segments = [
            Segment {
                label: "Record",
                enabled: !state.busy,
            },
            Segment {
                label: "Edit",
                enabled: state.can_edit && !state.busy,
            },
        ];
        if widgets::segmented::segmented(ui, &mut selected, &segments, 30.0) {
            action = Some(TitlebarAction::SetMode(if selected == 0 {
                Mode::Record
            } else {
                Mode::Edit
            }));
        }
    });
    action
}

fn right_section(ui: &mut Ui, bar_rect: Rect, state: &TitlebarState<'_>) -> Option<TitlebarAction> {
    let rect = bar_rect.shrink2(vec2(10.0, 0.0));
    let mut ui = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::right_to_left(Align::Center)),
    );
    if window_button(&mut ui, "\u{2715}", true) {
        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
    }
    if window_button(&mut ui, "\u{25a1}", false) {
        toggle_maximize(ui.ctx());
    }
    if window_button(&mut ui, "\u{2013}", false) {
        ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
    }
    ui.add_space(10.0);
    let export = widgets::button::primary(&mut ui, "Export", state.can_edit).clicked();
    ui.add_space(6.0);
    widgets::button::ghost(&mut ui, "Share", false).on_hover_text("Sharing lands post-MVP");
    export.then_some(TitlebarAction::Export)
}

/// 28px square icon button; `danger` gets the red close hover.
fn window_button(ui: &mut Ui, glyph: &str, danger: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    let (bg, fg) = if response.hovered() {
        if danger {
            (theme::RECORD_RED, Color32::WHITE)
        } else {
            (theme::BG_CONTROL_ACTIVE, theme::TEXT)
        }
    } else {
        (Color32::TRANSPARENT, theme::TEXT_MUTED)
    };
    ui.painter().rect_filled(rect, CornerRadius::same(6), bg);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::new(13.0, FontFamily::Proportional),
        fg,
    );
    response.clicked()
}

fn toggle_maximize(ctx: &Context) {
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
}
