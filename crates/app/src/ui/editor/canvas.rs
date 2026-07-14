//! Canvas: top strip (ratio, fit, history icons) and the composited preview
//! stage - wallpaper gradient, padded rounded frame with shadow, and the
//! player's video texture inside it.

use eframe::egui::{
    Align2, CentralPanel, Color32, CornerRadius, FontFamily, FontId, Frame, Rect, Shape, Stroke,
    StrokeKind, Ui, UiBuilder, epaint::RectShape, pos2, vec2,
};

use super::{EditorState, format_ns};
use crate::app::Session;
use crate::theme;
use crate::ui::widgets::{self, segmented::Segment};
use breez_core::project::RATIOS;
use breez_core::render::{self, ZoomView};

pub fn show(ui: &mut Ui, state: &mut EditorState, session: &mut Session) {
    CentralPanel::no_frame()
        .frame(Frame::new().fill(theme::BG_WINDOW))
        .show(ui, |ui| {
            top_strip(ui, state, session);
            stage(ui, state, session);
        });
}

fn top_strip(ui: &mut Ui, state: &mut EditorState, session: &mut Session) {
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
            state.dirty = true;
        }
        ui.add_space(8.0);
        widgets::button::ghost(ui, "Fit", false).on_hover_text("Preview zoom lands later");
        ui.with_layout(
            eframe::egui::Layout::right_to_left(eframe::egui::Align::Center),
            |ui| {
                widgets::button::icon(ui, "\u{26f6}", false)
                    .on_hover_text("Fullscreen preview lands later");
                widgets::button::icon(ui, "\u{21b7}", false).on_hover_text("Redo lands in Phase 7");
                widgets::button::icon(ui, "\u{21b6}", false).on_hover_text("Undo lands in Phase 7");
            },
        );
    });
}

fn stage(ui: &mut Ui, state: &EditorState, session: &Session) {
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
    match state.player.texture() {
        Some(texture) => {
            let view = zoom_view(state, session);
            let uv = uv_rect(view);
            ui.painter().add(Shape::Rect(
                RectShape::filled(frame, CornerRadius::same(radius), Color32::WHITE)
                    .with_texture(texture.id(), uv),
            ));
            ui.painter().rect_stroke(
                frame,
                CornerRadius::same(radius),
                Stroke::new(1.0, Color32::from_black_alpha(90)),
                StrokeKind::Inside,
            );
            if style.cursor.click_highlight {
                ripples(ui, state, session, frame, uv);
            }
            zoom_badge(ui, stage, view.level);
        }
        None => placeholder(ui, frame, radius, session, take),
    }
}

/// Animated zoom transform at the playhead: the frame rect stays put and
/// the texture UV window shrinks around the anchor (plan §5, preview =
/// UV/rect transform). Follow-cursor segments anchor on the take's clicks.
fn zoom_view(state: &EditorState, session: &Session) -> ZoomView {
    let timeline = &session.project.timeline;
    let duration = timeline.duration_ns();
    if duration == 0 {
        return ZoomView::NEUTRAL;
    }
    let t_ns = state.player.playhead_ns().min(duration.saturating_sub(1));
    let cursor = timeline.resolve(t_ns).and_then(|ct| {
        state
            .clicks
            .get(&ct.take)
            .and_then(|clicks| render::cursor_anchor(clicks, ct.src_ns))
    });
    render::zoom_at(timeline, t_ns, cursor)
}

/// UV window for a zoom view: size `1/level`, centered on the anchor but
/// clamped so it never samples outside the texture.
fn uv_rect(view: ZoomView) -> Rect {
    let half = 0.5 / view.level.max(1.0);
    Rect::from_center_size(
        pos2(
            view.anchor[0].clamp(half, 1.0 - half),
            view.anchor[1].clamp(half, 1.0 - half),
        ),
        vec2(half * 2.0, half * 2.0),
    )
}

/// Expanding stroked circles at recent click positions, mapped through the
/// zoom UV window so they stay glued to the pixels they were clicked on.
fn ripples(ui: &Ui, state: &EditorState, session: &Session, frame: Rect, uv: Rect) {
    let timeline = &session.project.timeline;
    let t_ns = state
        .player
        .playhead_ns()
        .min(timeline.duration_ns().saturating_sub(1));
    let Some(ct) = timeline.resolve(t_ns) else {
        return;
    };
    let Some(clicks) = state.clicks.get(&ct.take) else {
        return;
    };
    let painter = ui.painter_at(frame);
    for ripple in render::ripples_at(clicks, ct.src_ns) {
        let x = (ripple.x - uv.min.x) / uv.width();
        let y = (ripple.y - uv.min.y) / uv.height();
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            continue;
        }
        let center = pos2(
            frame.min.x + x * frame.width(),
            frame.min.y + y * frame.height(),
        );
        let radius = frame.width() * (0.006 + 0.022 * ripple.progress);
        let alpha = ((1.0 - ripple.progress) * 180.0) as u8;
        painter.circle_stroke(
            center,
            radius,
            Stroke::new(2.0, Color32::from_white_alpha(alpha)),
        );
    }
}

/// Current zoom level chip in the stage's top-right corner.
fn zoom_badge(ui: &Ui, stage: Rect, level: f32) {
    if level < 1.02 {
        return;
    }
    let text = format!("{level:.1}x");
    let galley =
        ui.painter()
            .layout_no_wrap(text, FontId::new(11.0, FontFamily::Monospace), theme::TEXT);
    let size = galley.size() + vec2(14.0, 8.0);
    let rect = Rect::from_min_size(pos2(stage.max.x - size.x - 12.0, stage.min.y + 12.0), size);
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::RADIUS_CHIP),
        Color32::from_black_alpha(150),
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, theme::TEXT);
}

/// Frame contents while the first preview frame is still decoding.
fn placeholder(
    ui: &Ui,
    frame: Rect,
    radius: u8,
    session: &Session,
    take: &breez_core::project::Take,
) {
    ui.painter().rect_filled(
        frame,
        CornerRadius::same(radius),
        Color32::from_rgb(0x05, 0x05, 0x05),
    );
    ui.painter().text(
        frame.center() - vec2(0.0, 12.0),
        Align2::CENTER_CENTER,
        &session.project.name,
        FontId::new(15.0, theme::semibold()),
        theme::TEXT,
    );
    ui.painter().text(
        frame.center() + vec2(0.0, 12.0),
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
