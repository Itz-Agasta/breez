//! Canvas: top strip (ratio, fit, history icons) and the composited preview
//! stage. Phase 2 paints the wallpaper + framed placeholder with the take
//! metadata; the video texture arrives with playback in Phase 3.

use eframe::egui::{
    Align2, CentralPanel, Color32, CornerRadius, FontFamily, FontId, Frame, Rect, Ui, UiBuilder,
    pos2, vec2,
};

use super::{RATIOS, format_ns};
use crate::app::Session;
use crate::theme;
use crate::ui::widgets::{self, segmented::Segment};

pub fn show(ui: &mut Ui, session: &mut Session) {
    CentralPanel::no_frame()
        .frame(Frame::new().fill(theme::BG_WINDOW))
        .show(ui, |ui| {
            top_strip(ui, session);
            stage(ui, session);
        });
}

fn top_strip(ui: &mut Ui, session: &mut Session) {
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme::CANVAS_STRIP_HEIGHT),
        eframe::egui::Sense::hover(),
    );
    let mut strip = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 0.0))));
    strip.horizontal_centered(|ui| {
        let mut ratio_idx = RATIOS
            .iter()
            .position(|r| *r == session.project.style.ratio)
            .unwrap_or(0);
        let segments: Vec<Segment> = RATIOS.iter().map(|r| Segment::new(r)).collect();
        if widgets::segmented::segmented(ui, &mut ratio_idx, &segments, 28.0) {
            session.project.style.ratio = RATIOS[ratio_idx].to_owned();
        }
        ui.add_space(8.0);
        widgets::button::ghost(ui, "Fit", false).on_hover_text("Zoom controls land in Phase 3");
        ui.with_layout(
            eframe::egui::Layout::right_to_left(eframe::egui::Align::Center),
            |ui| {
                widgets::button::icon(ui, "\u{26f6}", false)
                    .on_hover_text("Fullscreen preview lands in Phase 3");
                widgets::button::icon(ui, "\u{21b7}", false).on_hover_text("Redo lands in Phase 7");
                widgets::button::icon(ui, "\u{21b6}", false).on_hover_text("Undo lands in Phase 7");
            },
        );
    });
}

fn stage(ui: &mut Ui, session: &Session) {
    let outer = ui.available_rect_before_wrap().shrink2(vec2(16.0, 0.0));
    let bounds = Rect::from_min_max(outer.min, pos2(outer.max.x, outer.max.y - 16.0));
    if bounds.height() < 40.0 {
        return;
    }
    let style = &session.project.style;
    // The stage carries the output aspect ratio the user picked.
    let stage = fit_aspect(bounds, ratio_aspect(&style.ratio));
    let (top, bottom) = wallpaper_colors(&style.wallpaper);
    widgets::vertical_gradient(ui.painter(), stage, theme::RADIUS_CARD, top, bottom);

    let Some(take) = session.project.takes.last() else {
        return;
    };
    // Padding is authored against a 1440px-wide stage; scale it with the view.
    let inset = style.padding as f32 * stage.width() / 1440.0;
    let avail = stage.shrink(inset.max(8.0));
    let aspect = take.width.max(1) as f32 / take.height.max(1) as f32;
    let frame = fit_aspect(avail, aspect);

    let radius = style.radius.min(255) as u8;
    shadow(ui, frame, style.shadow, radius);
    ui.painter().rect_filled(
        frame,
        CornerRadius::same(radius),
        Color32::from_rgb(0x05, 0x05, 0x05),
    );
    ui.painter().text(
        frame.center() - vec2(0.0, 22.0),
        Align2::CENTER_CENTER,
        &session.project.name,
        FontId::new(15.0, theme::semibold()),
        theme::TEXT,
    );
    ui.painter().text(
        frame.center() + vec2(0.0, 2.0),
        Align2::CENTER_CENTER,
        format!(
            "{}x{} @ {}fps \u{00b7} {}",
            take.width,
            take.height,
            take.fps,
            format_ns(take.duration_ns)
        ),
        FontId::new(12.5, FontFamily::Monospace),
        theme::TEXT_MUTED,
    );
    ui.painter().text(
        frame.center() + vec2(0.0, 26.0),
        Align2::CENTER_CENTER,
        "Playback lands in Phase 3",
        FontId::new(11.0, FontFamily::Proportional),
        theme::TEXT_FAINT,
    );
}

/// Cheap drop shadow: a few expanding translucent layers under the frame.
fn shadow(ui: &Ui, frame: Rect, strength: u32, radius: u8) {
    if strength == 0 {
        return;
    }
    let alpha = (strength as f32 / 100.0 * 40.0) as u8;
    for (expand, layer_alpha) in [(3.0, alpha), (8.0, alpha / 2), (16.0, alpha / 4)] {
        ui.painter().rect_filled(
            frame.expand(expand).translate(vec2(0.0, expand / 2.0)),
            CornerRadius::same(radius.saturating_add(expand as u8)),
            Color32::from_black_alpha(layer_alpha),
        );
    }
}

/// Largest rect of the given aspect ratio centered inside `bounds`.
fn fit_aspect(bounds: Rect, aspect: f32) -> Rect {
    let size = if bounds.width() / bounds.height() > aspect {
        vec2(bounds.height() * aspect, bounds.height())
    } else {
        vec2(bounds.width(), bounds.width() / aspect)
    };
    Rect::from_center_size(bounds.center(), size)
}

fn ratio_aspect(ratio: &str) -> f32 {
    match ratio {
        "9:16" => 9.0 / 16.0,
        "1:1" => 1.0,
        _ => 16.0 / 9.0,
    }
}

pub fn wallpaper_colors(id: &str) -> (Color32, Color32) {
    theme::WALLPAPERS
        .iter()
        .find(|(name, _, _)| *name == id)
        .map(|(_, top, bottom)| (*top, *bottom))
        .unwrap_or((theme::WALLPAPERS[0].1, theme::WALLPAPERS[0].2))
}
