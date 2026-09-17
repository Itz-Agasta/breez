//! CPU frame compositor: wallpaper gradient, drop shadow, rounded video
//! frame, zoom window, click ripples.
//!
//! Layer order matches the egui preview in `app::ui::editor::canvas`, and
//! every rect comes from `breez_core::layout`, so the two agree by
//! construction rather than by being kept in sync by hand. Rows are painted
//! in parallel with rayon and the output buffer is reused across frames, so
//! a long export does not churn allocations.

use breez_core::layout::{self, Layout, Rect};
use breez_core::project::Style;
use breez_core::render::{Ripple, ZoomView};
use breez_core::wallpaper;
use rayon::prelude::*;

use crate::raster::{lerp_rgb, rounded_coverage, sample_bilinear};

/// One decoded source frame: tightly packed RGBA8.
pub struct SourceFrame<'a> {
    pub data: &'a [u8],
    pub width: u32,
    pub height: u32,
}

/// Everything that varies per output frame.
pub struct FrameParams<'a> {
    pub style: &'a Style,
    pub zoom: ZoomView,
    pub ripples: &'a [Ripple],
    pub click_highlight: bool,
}

pub struct Compositor {
    width: u32,
    height: u32,
    buffer: Vec<u8>,
}

impl Compositor {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            buffer: vec![0; (width as usize) * (height as usize) * 4],
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Composite one frame. The returned RGBA8 slice stays valid until the
    /// next call.
    pub fn compose(&mut self, source: SourceFrame<'_>, params: &FrameParams<'_>) -> &[u8] {
        let stage = Rect::from_size(self.width as f32, self.height as f32);
        let out = layout::layout(params.style, stage, source.width, source.height);
        let uv = layout::uv_window(params.zoom);
        let (top, bottom) = wallpaper::wallpaper_colors(&params.style.wallpaper);
        let ripples = if params.click_highlight {
            ripple_draws(params.ripples, &out, uv)
        } else {
            Vec::new()
        };
        // The hairline the preview strokes 1px inside the frame edge.
        let hairline = out.frame.shrink(out.scale.max(0.5));
        let hairline_radius = (out.radius - out.scale).max(0.0);
        let shadow_bounds = shadow_bounds(&out);
        let height = self.height as f32;

        self.buffer
            .par_chunks_mut((self.width as usize) * 4)
            .enumerate()
            .for_each(|(row, line)| {
                let py = row as f32 + 0.5;
                let bg = lerp_rgb(top, bottom, py / height);
                for (col, pixel) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let px = col as f32 + 0.5;
                    let coverage = rounded_coverage(out.frame, out.radius, px, py);
                    // The shadow only shows where the frame does not fully
                    // cover the pixel, and it is the most expensive layer, so
                    // the majority of pixels skip it entirely.
                    let mut rgb = if coverage < 1.0 {
                        blend(bg, [0, 0, 0], shadow_alpha(&out, &shadow_bounds, px, py))
                    } else {
                        bg
                    };
                    if coverage > 0.0 {
                        let u = uv.x + (px - out.frame.x) / out.frame.w * uv.w;
                        let v = uv.y + (py - out.frame.y) / out.frame.h * uv.h;
                        let sample = sample_bilinear(
                            source.data,
                            source.width,
                            source.height,
                            u * source.width as f32 - 0.5,
                            v * source.height as f32 - 0.5,
                        );
                        rgb = blend(rgb, sample, coverage);
                        let inner = rounded_coverage(hairline, hairline_radius, px, py);
                        rgb = blend(rgb, [0, 0, 0], (coverage - inner).max(0.0) * (90.0 / 255.0));
                        for ripple in &ripples {
                            rgb = blend(rgb, [255, 255, 255], ripple.alpha_at(px, py));
                        }
                    }
                    pixel[0] = rgb[0];
                    pixel[1] = rgb[1];
                    pixel[2] = rgb[2];
                    pixel[3] = 0xff;
                }
            });
        &self.buffer
    }
}

/// A ripple resolved to output pixels.
struct RippleDraw {
    cx: f32,
    cy: f32,
    radius: f32,
    half_stroke: f32,
    alpha: f32,
}

impl RippleDraw {
    fn alpha_at(&self, px: f32, py: f32) -> f32 {
        let edge = ((px - self.cx).hypot(py - self.cy) - self.radius).abs();
        if edge > self.half_stroke + 0.5 {
            return 0.0;
        }
        self.alpha * (self.half_stroke + 0.5 - edge).clamp(0.0, 1.0)
    }
}

/// Map `render::ripples_at` output through the zoom window into output
/// pixels, dropping any ripple the zoom has cropped away. Mirrors
/// `canvas::ripples`.
fn ripple_draws(ripples: &[Ripple], out: &Layout, uv: Rect) -> Vec<RippleDraw> {
    ripples
        .iter()
        .filter_map(|ripple| {
            let x = (ripple.x - uv.x) / uv.w;
            let y = (ripple.y - uv.y) / uv.h;
            if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
                return None;
            }
            Some(RippleDraw {
                cx: out.frame.x + x * out.frame.w,
                cy: out.frame.y + y * out.frame.h,
                radius: out.frame.w * (0.006 + 0.022 * ripple.progress),
                half_stroke: out.scale.max(0.5),
                alpha: (1.0 - ripple.progress) * (180.0 / 255.0),
            })
        })
        .collect()
}

/// Shadow alpha at a pixel: the same three expanding translucent layers the
/// preview paints, stacked the way overlapping translucent fills stack.
fn shadow_alpha(out: &Layout, bounds: &Rect, px: f32, py: f32) -> f32 {
    if out.shadow == 0
        || px < bounds.x
        || py < bounds.y
        || px > bounds.max_x()
        || py > bounds.max_y()
    {
        return 0.0;
    }
    let base = out.shadow as f32 / 100.0 * 40.0;
    let mut alpha = 0.0f32;
    for (expand, layer) in [(3.0, base), (8.0, base / 2.0), (16.0, base / 4.0)] {
        let expand = expand * out.scale;
        let rect = Rect {
            x: out.frame.x - expand,
            y: out.frame.y - expand + expand / 2.0,
            w: out.frame.w + 2.0 * expand,
            h: out.frame.h + 2.0 * expand,
        };
        let coverage = rounded_coverage(rect, out.radius + expand, px, py);
        alpha += (1.0 - alpha) * coverage * (layer.floor() / 255.0);
    }
    alpha
}

/// Bounding box of the outermost shadow layer, so pixels nowhere near the
/// frame can skip the three coverage tests.
fn shadow_bounds(out: &Layout) -> Rect {
    let expand = 16.0 * out.scale;
    Rect {
        x: out.frame.x - expand - 1.0,
        y: out.frame.y - expand / 2.0 - 1.0,
        w: out.frame.w + 2.0 * expand + 2.0,
        h: out.frame.h + 2.0 * expand + 2.0,
    }
}

/// Source-over blend of `over` onto `under` at `alpha`.
fn blend(under: [u8; 3], over: [u8; 3], alpha: f32) -> [u8; 3] {
    if alpha <= 0.0 {
        return under;
    }
    lerp_rgb(under, over, alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 64x36 solid red RGBA source, a 16:9 take.
    fn red_source() -> Vec<u8> {
        [0xffu8, 0x00, 0x00, 0xff].repeat(64 * 36)
    }

    fn params(style: &Style) -> FrameParams<'_> {
        FrameParams {
            style,
            zoom: ZoomView::NEUTRAL,
            ripples: &[],
            click_highlight: false,
        }
    }

    fn compose(style: &Style, data: &[u8], w: u32, h: u32, out: (u32, u32)) -> Vec<u8> {
        let mut compositor = Compositor::new(out.0, out.1);
        compositor
            .compose(
                SourceFrame {
                    data,
                    width: w,
                    height: h,
                },
                &params(style),
            )
            .to_vec()
    }

    fn pixel(buf: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * width + x) * 4) as usize;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    fn mono() -> Style {
        Style {
            wallpaper: "mono".to_owned(),
            ..Style::default()
        }
    }

    #[test]
    fn compose_should_fill_the_whole_output_buffer() {
        let out = compose(&Style::default(), &red_source(), 64, 36, (320, 180));
        assert_eq!(out.len(), 320 * 180 * 4);
    }

    #[test]
    fn compose_should_paint_the_source_at_the_frame_center() {
        let out = compose(&Style::default(), &red_source(), 64, 36, (320, 180));
        assert_eq!(pixel(&out, 320, 160, 90), [0xff, 0x00, 0x00, 0xff]);
    }

    #[test]
    fn compose_should_paint_the_gradient_top_at_the_stage_top() {
        let out = compose(&mono(), &red_source(), 64, 36, (320, 180));
        assert_eq!(pixel(&out, 320, 160, 0), [0x26, 0x26, 0x26, 0xff]);
    }

    #[test]
    fn compose_should_paint_the_gradient_bottom_at_the_stage_bottom() {
        let out = compose(&mono(), &red_source(), 64, 36, (320, 180));
        assert_eq!(pixel(&out, 320, 160, 179), [0x0f, 0x0f, 0x0f, 0xff]);
    }

    #[test]
    fn compose_should_be_fully_opaque_everywhere() {
        let out = compose(&Style::default(), &red_source(), 64, 36, (64, 36));
        assert!(out.as_chunks::<4>().0.iter().all(|px| px[3] == 0xff));
    }

    #[test]
    fn compose_should_zoom_into_the_anchor() {
        // Left half blue, right half red: a 2x zoom anchored hard left must
        // put blue at the frame center.
        let (w, h) = (64u32, 36u32);
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..h {
            for x in 0..w {
                data.extend_from_slice(if x < w / 2 {
                    &[0x00, 0x00, 0xff, 0xff]
                } else {
                    &[0xff, 0x00, 0x00, 0xff]
                });
            }
        }
        let style = Style {
            radius: 0,
            shadow: 0,
            ..Style::default()
        };
        let mut compositor = Compositor::new(320, 180);
        let out = compositor.compose(
            SourceFrame {
                data: &data,
                width: w,
                height: h,
            },
            &FrameParams {
                style: &style,
                zoom: ZoomView {
                    level: 2.0,
                    anchor: [0.0, 0.5],
                },
                ripples: &[],
                click_highlight: false,
            },
        );
        assert_eq!(pixel(out, 320, 160, 90), [0x00, 0x00, 0xff, 0xff]);
    }

    #[test]
    fn compose_should_darken_under_the_frame_when_shadow_is_on() {
        let lit = Style {
            shadow: 0,
            ..mono()
        };
        let shadowed = Style {
            shadow: 100,
            ..mono()
        };
        let source = red_source();
        // A 1440-wide stage is the design reference, so scale is 1.0 and the
        // shadow spreads its authored 16px rather than a rounded-off sliver.
        let without = compose(&lit, &source, 64, 36, (1440, 810));
        let with = compose(&shadowed, &source, 64, 36, (1440, 810));
        // Just under the frame's bottom edge, still on the wallpaper.
        let probe = |buf: &[u8]| pixel(buf, 1440, 720, 755)[0];
        assert!(probe(&with) < probe(&without));
    }

    #[test]
    fn compose_should_ignore_ripples_when_click_highlight_is_off() {
        let style = Style::default();
        let source = red_source();
        let ripple = Ripple {
            x: 0.5,
            y: 0.5,
            progress: 0.5,
        };
        let mut compositor = Compositor::new(320, 180);
        let off = compositor
            .compose(
                SourceFrame {
                    data: &source,
                    width: 64,
                    height: 36,
                },
                &FrameParams {
                    style: &style,
                    zoom: ZoomView::NEUTRAL,
                    ripples: &[ripple],
                    click_highlight: false,
                },
            )
            .to_vec();
        assert_eq!(off, compose(&style, &source, 64, 36, (320, 180)));
    }
}
