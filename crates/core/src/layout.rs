//! Output geometry shared by the egui preview and the export compositor.
//!
//! Everything the design authors in pixels is authored against a 1440px-wide
//! stage (plan.md §5 works from a 1440x884 reference), so padding, corner
//! radius, shadow offsets and hairlines all scale by `stage.w / 1440`. Both
//! renderers take their rects from [`layout`], so preview and export cannot
//! drift apart as the window or the export resolution changes.

use crate::project::Style;
use crate::render::ZoomView;

/// Reference stage width every pixel value in the design is authored against.
const REFERENCE_WIDTH: f32 = 1440.0;

/// Axis-aligned rect in output pixels, or in 0..1 UV units for [`uv_window`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn from_size(w: f32, h: f32) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w,
            h,
        }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn max_x(&self) -> f32 {
        self.x + self.w
    }

    pub fn max_y(&self) -> f32 {
        self.y + self.h
    }

    /// Inset on all four sides, never past zero size.
    pub fn shrink(&self, by: f32) -> Self {
        let by = by.min(self.w / 2.0).min(self.h / 2.0).max(0.0);
        Self {
            x: self.x + by,
            y: self.y + by,
            w: self.w - 2.0 * by,
            h: self.h - 2.0 * by,
        }
    }
}

/// Where every layer of one composited frame goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// Wallpaper area, carrying the output aspect ratio.
    pub stage: Rect,
    /// Video frame inside the stage.
    pub frame: Rect,
    /// Corner radius in output pixels, already scaled.
    pub radius: f32,
    /// Shadow strength 0..=100, straight from the style.
    pub shadow: u32,
    /// Multiplier for anything else authored in pixels.
    pub scale: f32,
}

/// Aspect ratio for a `style.ratio` id; unknown ids fall back to 16:9.
pub fn ratio_aspect(ratio: &str) -> f32 {
    match ratio {
        "9:16" => 9.0 / 16.0,
        "1:1" => 1.0,
        _ => 16.0 / 9.0,
    }
}

/// Largest rect of `aspect` centered inside `bounds`.
pub fn fit_aspect(bounds: Rect, aspect: f32) -> Rect {
    let aspect = aspect.max(f32::EPSILON);
    let (w, h) = if bounds.w / bounds.h.max(f32::EPSILON) > aspect {
        (bounds.h * aspect, bounds.h)
    } else {
        (bounds.w, bounds.w / aspect)
    };
    let (cx, cy) = bounds.center();
    Rect {
        x: cx - w / 2.0,
        y: cy - h / 2.0,
        w,
        h,
    }
}

/// Resolve the composited geometry for a take drawn on `stage`.
pub fn layout(style: &Style, stage: Rect, take_w: u32, take_h: u32) -> Layout {
    let scale = stage.w / REFERENCE_WIDTH;
    // The 8px floor keeps a hairline of wallpaper visible on a tiny preview
    // stage, where the scaled padding would otherwise round to nothing.
    let inset = (style.padding as f32 * scale).max(8.0);
    let avail = stage.shrink(inset);
    let aspect = take_w.max(1) as f32 / take_h.max(1) as f32;
    Layout {
        stage,
        frame: fit_aspect(avail, aspect),
        radius: style.radius as f32 * scale,
        shadow: style.shadow,
        scale,
    }
}

/// Texture window for a zoom view: size `1/level`, centered on the anchor and
/// clamped so it never samples outside the frame.
pub fn uv_window(view: ZoomView) -> Rect {
    let half = 0.5 / view.level.max(1.0);
    Rect {
        x: view.anchor[0].clamp(half, 1.0 - half) - half,
        y: view.anchor[1].clamp(half, 1.0 - half) - half,
        w: half * 2.0,
        h: half * 2.0,
    }
}

/// Export dimensions: the preset lands on the short side and the ratio sets
/// the other. Both are rounded down to even, which yuv420p requires.
pub fn output_size(ratio: &str, short_side: u32) -> (u32, u32) {
    let aspect = ratio_aspect(ratio);
    let (w, h) = if aspect >= 1.0 {
        ((short_side as f32 * aspect).round() as u32, short_side)
    } else {
        (short_side, (short_side as f32 / aspect).round() as u32)
    };
    (w & !1, h & !1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Style;

    fn style() -> Style {
        Style {
            padding: 64,
            radius: 14,
            shadow: 50,
            ratio: "16:9".to_owned(),
            ..Style::default()
        }
    }

    #[test]
    fn fit_aspect_should_letterbox_a_wide_bounds() {
        let r = fit_aspect(Rect::from_size(400.0, 100.0), 1.0);
        assert_eq!((r.x, r.y, r.w, r.h), (150.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn layout_should_scale_the_radius_with_the_stage_width() {
        // 2880 is 2x the 1440px reference the design is authored against.
        let out = layout(&style(), Rect::from_size(2880.0, 1620.0), 1920, 1080);
        assert_eq!(out.scale, 2.0);
        assert_eq!(out.radius, 28.0);
    }

    #[test]
    fn layout_should_scale_the_padding_with_the_stage_width() {
        let out = layout(&style(), Rect::from_size(2880.0, 1620.0), 1920, 1080);
        // 64px padding at 2x insets 128px off each edge; the take is wider
        // than the inset area, so the frame ends up height-limited.
        assert_eq!(out.frame.h, 1620.0 - 2.0 * 128.0);
    }

    #[test]
    fn layout_should_center_the_frame_in_the_stage() {
        let stage = Rect {
            x: 10.0,
            y: 20.0,
            w: 1440.0,
            h: 810.0,
        };
        let out = layout(&style(), stage, 1920, 1080);
        assert_eq!(out.frame.center(), stage.center());
    }

    #[test]
    fn uv_window_should_clamp_inside_the_texture() {
        let uv = uv_window(ZoomView {
            level: 2.0,
            anchor: [0.0, 1.0],
        });
        assert_eq!((uv.x, uv.y, uv.w, uv.h), (0.0, 0.5, 0.5, 0.5));
    }

    #[test]
    fn uv_window_should_be_the_full_frame_at_level_one() {
        let uv = uv_window(ZoomView::NEUTRAL);
        assert_eq!((uv.x, uv.y, uv.w, uv.h), (0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn output_size_should_put_the_preset_on_the_short_side() {
        assert_eq!(output_size("16:9", 1080), (1920, 1080));
        assert_eq!(output_size("9:16", 1080), (1080, 1920));
        assert_eq!(output_size("1:1", 1440), (1440, 1440));
    }

    #[test]
    fn output_size_should_round_down_to_even_dimensions() {
        assert_eq!(output_size("16:9", 719), (1278, 718));
    }
}
