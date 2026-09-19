//! Transport bar: prev/play/next drive the player, mono time readout shows
//! playhead / duration. Split and zoom-keyframe tools stay stubs until
//! their phases.

use eframe::egui::{
    Align, Align2, Color32, FontFamily, FontId, Layout, Sense, Ui, UiBuilder, vec2,
};

use super::{EditorState, keyframes};
use crate::app::Session;
use crate::theme;
use crate::ui::editor::format_ns;
use crate::ui::widgets;

pub fn show(ui: &mut Ui, state: &mut EditorState, session: &mut Session) {
    let duration_ns = session.project.timeline.duration_ns();
    let has_media = duration_ns > 0;
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme::TRANSPORT_HEIGHT),
        Sense::hover(),
    );
    let mut bar = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 0.0))));
    bar.horizontal_centered(|ui| {
        if widgets::button::icon(ui, "\u{23ee}", has_media).clicked() {
            state.player.seek(session, 0);
        }
        if play_button(ui, state.player.is_playing(), has_media).clicked() {
            state.player.toggle(session);
        }
        if widgets::button::icon(ui, "\u{23ed}", has_media).clicked() {
            state.player.seek(session, duration_ns);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            widgets::button::ghost(ui, "Fit", false).on_hover_text("Timeline zoom lands later");
            ui.add_space(4.0);
            widgets::button::icon(ui, "\u{1f50a}", false)
                .on_hover_text("System audio gain lives in the Audio inspector");
            if widgets::button::icon(ui, "\u{25c6}", has_media)
                .on_hover_text("Add zoom keyframe at the playhead")
                .clicked()
            {
                keyframes::add_at(state, session, state.player.playhead_ns());
            }
            widgets::button::icon(ui, "\u{2702}", false).on_hover_text("Split lands later");
        });
    });
    // Centered time readout painted over the bar so button layout can't shift it.
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        format!(
            "{} / {}",
            format_ns(state.player.playhead_ns()),
            format_ns(duration_ns)
        ),
        FontId::new(12.5, FontFamily::Monospace),
        theme::TEXT_MUTED,
    );
}

fn play_button(ui: &mut Ui, playing: bool, enabled: bool) -> eframe::egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(34.0, 34.0), sense);
    let hovered = enabled && response.hovered();
    let bg = if !enabled {
        theme::BG_CONTROL_ACTIVE
    } else if hovered {
        Color32::from_rgb(0xd9, 0xd9, 0xd9)
    } else {
        theme::ACCENT
    };
    ui.painter().circle_filled(rect.center(), 17.0, bg);
    let (glyph, nudge) = if playing {
        ("\u{23f8}", 0.0)
    } else {
        ("\u{25b6}", 1.0)
    };
    ui.painter().text(
        rect.center() + vec2(nudge, 0.0),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(13.0),
        if enabled {
            Color32::BLACK
        } else {
            Color32::from_rgb(0x6a, 0x6a, 0x6a)
        },
    );
    response
}
