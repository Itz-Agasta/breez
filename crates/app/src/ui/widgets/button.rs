//! Text pills (primary/ghost) and square icon buttons. Disabled widgets
//! render faint, never hover, and never report clicks (hover sense only,
//! so tooltips explaining why still work).

use eframe::egui::{
    Align2, Color32, CornerRadius, FontId, Response, Sense, Stroke, StrokeKind, Ui, vec2,
};

use crate::theme;

/// White filled pill, black text. The main call-to-action.
pub fn primary(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    pill(ui, label, PillKind::Primary, enabled)
}

/// Bordered pill on transparent background.
pub fn ghost(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    pill(ui, label, PillKind::Ghost, enabled)
}

enum PillKind {
    Primary,
    Ghost,
}

fn pill(ui: &mut Ui, label: &str, kind: PillKind, enabled: bool) -> Response {
    let font = FontId::new(12.5, theme::medium());
    let text_width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), Color32::PLACEHOLDER)
        .size()
        .x;
    let (rect, response) = ui.allocate_exact_size(vec2(text_width + 26.0, 28.0), sense(enabled));
    let hovered = enabled && response.hovered();
    let (bg, fg, border) = match kind {
        PillKind::Primary => (
            if !enabled {
                theme::BG_CONTROL_ACTIVE
            } else if hovered {
                Color32::from_rgb(0xd9, 0xd9, 0xd9)
            } else {
                theme::ACCENT
            },
            if enabled {
                Color32::BLACK
            } else {
                theme::TEXT_LABEL
            },
            None,
        ),
        PillKind::Ghost => (
            if hovered {
                theme::BG_CONTROL_HOVER
            } else {
                Color32::TRANSPARENT
            },
            if enabled {
                theme::TEXT
            } else {
                theme::TEXT_LABEL
            },
            Some(if enabled {
                theme::BORDER_STRONG
            } else {
                theme::BORDER
            }),
        ),
    };
    let radius = CornerRadius::same(theme::RADIUS_BUTTON);
    ui.painter().rect_filled(rect, radius, bg);
    if let Some(border) = border {
        ui.painter()
            .rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    }
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, label, font, fg);
    response
}

/// Square icon button (28px), glyph rendered from the font fallback chain.
pub fn icon(ui: &mut Ui, glyph: &str, enabled: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), sense(enabled));
    let hovered = enabled && response.hovered();
    let bg = if hovered {
        theme::BG_CONTROL_HOVER
    } else {
        Color32::TRANSPARENT
    };
    let fg = if !enabled {
        theme::TEXT_LABEL
    } else if hovered {
        theme::TEXT
    } else {
        theme::TEXT_MUTED
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(theme::RADIUS_BUTTON), bg);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(13.0),
        fg,
    );
    response
}

fn sense(enabled: bool) -> Sense {
    if enabled {
        Sense::click()
    } else {
        Sense::hover()
    }
}
