//! Design tokens and egui style installation.
//!
//! Single source of truth for colors, radii, and fonts. UI code imports
//! constants from here instead of hardcoding values.

// Tokens land ahead of the widgets that use them (per plan phase order).
// Drop this once the editor UI is in place.
#![allow(dead_code)]

use eframe::egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke, Visuals,
};

pub const BG_WINDOW: Color32 = Color32::from_rgb(0x0a, 0x0a, 0x0a);
pub const BG_TITLEBAR: Color32 = Color32::from_rgb(0x0c, 0x0c, 0x0c);
pub const BG_PANEL_DARK: Color32 = Color32::from_rgb(0x08, 0x08, 0x08);
pub const BG_PANEL: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0b);
pub const BG_CONTROL: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);
pub const BG_CONTROL_HOVER: Color32 = Color32::from_rgb(0x17, 0x17, 0x17);
pub const BG_CONTROL_ACTIVE: Color32 = Color32::from_rgb(0x24, 0x24, 0x24);

pub const BORDER: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x24, 0x24, 0x24);

pub const TEXT: Color32 = Color32::from_rgb(0xed, 0xed, 0xed);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8f, 0x8f, 0x8f);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x7a, 0x7a, 0x7a);
pub const TEXT_LABEL: Color32 = Color32::from_rgb(0x56, 0x56, 0x56);

pub const ACCENT: Color32 = Color32::WHITE;
pub const RECORD_RED: Color32 = Color32::from_rgb(0xe5, 0x48, 0x4d);
pub const MUSIC_BLUE: Color32 = Color32::from_rgb(0x4a, 0x9e, 0xff);
pub const OK_GREEN: Color32 = Color32::from_rgb(0x28, 0xc8, 0x40);

pub const RADIUS_BUTTON: u8 = 7;
pub const RADIUS_CARD: u8 = 11;
pub const RADIUS_CLIP: u8 = 8;
pub const RADIUS_CHIP: u8 = 5;

pub const TITLEBAR_HEIGHT: f32 = 46.0;
pub const RAIL_WIDTH: f32 = 60.0;
pub const TOOLPANEL_WIDTH: f32 = 302.0;
pub const INSPECTOR_WIDTH: f32 = 328.0;
pub const TIMELINE_HEIGHT: f32 = 250.0;
pub const CANVAS_STRIP_HEIGHT: f32 = 44.0;
pub const TRANSPORT_HEIGHT: f32 = 48.0;
pub const GUTTER_WIDTH: f32 = 96.0;

/// egui view over [`breez_core::wallpaper::WALLPAPERS`]. The palette itself
/// lives in core so the export compositor reads the same stops.
pub fn color32(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

pub fn wallpapers() -> impl Iterator<Item = (&'static str, Color32, Color32)> {
    breez_core::wallpaper::WALLPAPERS
        .iter()
        .map(|(id, top, bottom)| (*id, color32(*top), color32(*bottom)))
}

/// Semibold proportional family, registered in [`fonts`].
pub fn semibold() -> FontFamily {
    FontFamily::Name("geist-semibold".into())
}

/// Medium proportional family, registered in [`fonts`].
pub fn medium() -> FontFamily {
    FontFamily::Name("geist-medium".into())
}

pub fn install(ctx: &Context) {
    ctx.set_fonts(fonts());
    ctx.set_visuals(visuals());
}

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let faces: &[(&str, &'static [u8])] = &[
        (
            "geist",
            include_bytes!("../../../assets/fonts/Geist-Regular.ttf"),
        ),
        (
            "geist-medium",
            include_bytes!("../../../assets/fonts/Geist-Medium.ttf"),
        ),
        (
            "geist-semibold",
            include_bytes!("../../../assets/fonts/Geist-SemiBold.ttf"),
        ),
        (
            "geist-bold",
            include_bytes!("../../../assets/fonts/Geist-Bold.ttf"),
        ),
        (
            "geist-mono",
            include_bytes!("../../../assets/fonts/GeistMono-Regular.ttf"),
        ),
        (
            "geist-mono-medium",
            include_bytes!("../../../assets/fonts/GeistMono-Medium.ttf"),
        ),
    ];
    for (name, bytes) in faces {
        fonts.font_data.insert(
            (*name).to_owned(),
            std::sync::Arc::new(FontData::from_static(bytes)),
        );
    }
    for (family, primary) in [
        (FontFamily::Proportional, "geist"),
        (FontFamily::Monospace, "geist-mono"),
    ] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, primary.to_owned());
    }
    for name in [
        "geist-medium",
        "geist-semibold",
        "geist-bold",
        "geist-mono-medium",
    ] {
        // Named families so UI code can pick a weight; fall back to regular
        // for glyphs missing from that face.
        fonts.families.insert(
            FontFamily::Name(name.into()),
            vec![name.to_owned(), "geist".to_owned()],
        );
    }
    fonts
}

fn visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = BG_WINDOW;
    v.window_fill = BG_WINDOW;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT.linear_multiply(0.25);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.bg_fill = BG_CONTROL;
    v.widgets.inactive.corner_radius = CornerRadius::same(RADIUS_BUTTON);
    v.widgets.hovered.bg_fill = BG_CONTROL_HOVER;
    v.widgets.hovered.corner_radius = CornerRadius::same(RADIUS_BUTTON);
    v.widgets.active.bg_fill = BG_CONTROL_ACTIVE;
    v.widgets.active.corner_radius = CornerRadius::same(RADIUS_BUTTON);
    v
}
