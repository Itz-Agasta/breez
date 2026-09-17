//! Pixel primitives for the compositor: gradient stops, the rounded-rect
//! coverage mask, and bilinear sampling of the source frame.
//!
//! Kept apart from `compose` so each piece is small enough to test on its
//! own; the compositor is then just layer order plus these three.

use breez_core::layout::Rect;

/// Linear interpolation between two sRGB triples. The preview lerps the
/// wallpaper in gamma space (egui's `lerp_to_gamma`), so matching that
/// matters more here than being colorimetrically correct.
pub fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    // `+ 0.5` then truncate rounds half-up like `round()` does for the 0..255
    // range these always sit in, and avoids a libm call per component.
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t + 0.5) as u8;
    [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
}

/// Coverage (0..=1) of the pixel at (`px`, `py`) by `rect` with corner
/// `radius`, anti-aliased over a one pixel band so exported corners are as
/// smooth as the preview's.
pub fn rounded_coverage(rect: Rect, radius: f32, px: f32, py: f32) -> f32 {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return 0.0;
    }
    let radius = radius.max(0.0).min(rect.w / 2.0).min(rect.h / 2.0);
    let (cx, cy) = rect.center();
    // Signed distance to the rounded rect's surface, negative inside.
    let dx = (px - cx).abs() - (rect.w / 2.0 - radius);
    let dy = (py - cy).abs() - (rect.h / 2.0 - radius);
    let outside = dx.max(0.0).hypot(dy.max(0.0));
    let inside = dx.max(dy).min(0.0);
    let distance = outside + inside - radius;
    (0.5 - distance).clamp(0.0, 1.0)
}

/// Horizontal span of row `py` over which [`rounded_coverage`] is exactly
/// `1.0`, or `None` when the row has no fully covered pixels.
///
/// The compositor uses this to skip the two distance evaluations for the
/// bulk of the frame, which is the difference between an export that keeps
/// up and one that does not.
pub fn full_coverage_span(rect: Rect, radius: f32, py: f32) -> Option<(f32, f32)> {
    if rect.w <= 1.0 || rect.h <= 1.0 {
        return None;
    }
    let radius = radius.max(0.0).min(rect.w / 2.0).min(rect.h / 2.0);
    let (cx, cy) = rect.center();
    let straight_h = rect.h / 2.0 - radius;
    let dy = (py - cy).abs() - straight_h;
    // Inside the straight section the corner arcs do not reach this row.
    let half = if dy <= 0.0 {
        rect.w / 2.0 - 0.5
    } else {
        let reach = radius - 0.5;
        if dy > reach {
            return None;
        }
        rect.w / 2.0 - radius + (reach * reach - dy * dy).sqrt()
    };
    (half > 0.0).then_some((cx - half, cx + half))
}

/// Bilinear RGB sample of a tightly packed RGBA8 buffer at pixel
/// coordinates, clamped at the edges.
pub fn sample_bilinear(src: &[u8], width: u32, height: u32, x: f32, y: f32) -> [u8; 3] {
    if width == 0 || height == 0 || src.len() < (width as usize) * (height as usize) * 4 {
        return [0, 0, 0];
    }
    let max_x = (width - 1) as f32;
    let max_y = (height - 1) as f32;
    let x = x.clamp(0.0, max_x);
    let y = y.clamp(0.0, max_y);
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let x1 = (x0 + 1.0).min(max_x);
    let y1 = (y0 + 1.0).min(max_y);
    let at = |cx: f32, cy: f32| {
        let i = ((cy as usize) * (width as usize) + cx as usize) * 4;
        [src[i], src[i + 1], src[i + 2]]
    };
    let top = lerp_rgb(at(x0, y0), at(x1, y0), fx);
    let bottom = lerp_rgb(at(x0, y1), at(x1, y1), fx);
    lerp_rgb(top, bottom, fy)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Rect {
        Rect::from_size(100.0, 100.0)
    }

    #[test]
    fn lerp_rgb_should_return_the_endpoints() {
        assert_eq!(lerp_rgb([0, 0, 0], [255, 255, 255], 0.0), [0, 0, 0]);
        assert_eq!(lerp_rgb([0, 0, 0], [255, 255, 255], 1.0), [255, 255, 255]);
    }

    #[test]
    fn rounded_coverage_should_be_one_well_inside_the_rect() {
        assert_eq!(rounded_coverage(square(), 20.0, 50.0, 50.0), 1.0);
    }

    #[test]
    fn rounded_coverage_should_be_zero_outside_a_rounded_corner() {
        // (1,1) sits outside the arc of a 20px corner centered at (20,20).
        assert_eq!(rounded_coverage(square(), 20.0, 1.0, 1.0), 0.0);
    }

    #[test]
    fn rounded_coverage_should_be_one_in_a_square_corner() {
        assert_eq!(rounded_coverage(square(), 0.0, 1.0, 1.0), 1.0);
    }

    #[test]
    fn rounded_coverage_should_be_zero_outside_the_rect() {
        assert_eq!(rounded_coverage(square(), 0.0, 150.0, 50.0), 0.0);
    }

    #[test]
    fn full_coverage_span_should_cover_the_row_minus_a_half_pixel_in_the_straight_section() {
        let (lo, hi) = full_coverage_span(square(), 20.0, 50.0).expect("span");
        assert_eq!((lo, hi), (0.5, 99.5));
    }

    #[test]
    fn full_coverage_span_should_narrow_across_a_corner_row() {
        let (lo, hi) = full_coverage_span(square(), 20.0, 5.0).expect("span");
        assert!(lo > 0.5 && hi < 99.5);
    }

    #[test]
    fn full_coverage_span_should_be_empty_past_the_corner_arc() {
        assert_eq!(full_coverage_span(square(), 20.0, 0.0), None);
    }

    #[test]
    fn full_coverage_span_should_agree_with_rounded_coverage() {
        let rect = Rect {
            x: 3.0,
            y: 7.0,
            w: 81.0,
            h: 47.0,
        };
        for row in 0..60 {
            let py = row as f32 + 0.5;
            let span = full_coverage_span(rect, 11.0, py);
            for col in 0..90 {
                let px = col as f32 + 0.5;
                let full = rounded_coverage(rect, 11.0, px, py) >= 1.0;
                let in_span = span.is_some_and(|(lo, hi)| px >= lo && px <= hi);
                assert_eq!(full, in_span, "at ({px}, {py})");
            }
        }
    }

    #[test]
    fn sample_bilinear_should_read_a_flat_source_exactly() {
        let src = [0x20u8, 0x40, 0x60, 0xff].repeat(4);
        assert_eq!(sample_bilinear(&src, 2, 2, 0.5, 0.5), [0x20, 0x40, 0x60]);
    }

    #[test]
    fn sample_bilinear_should_clamp_out_of_range_coordinates() {
        let src = [0x20u8, 0x40, 0x60, 0xff].repeat(4);
        assert_eq!(sample_bilinear(&src, 2, 2, -3.0, 9.0), [0x20, 0x40, 0x60]);
    }

    #[test]
    fn sample_bilinear_should_blend_between_neighbours() {
        // Two columns: black then white, sampled exactly halfway.
        let src = [0u8, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xff].repeat(2);
        assert_eq!(sample_bilinear(&src, 2, 2, 0.5, 0.0), [128, 128, 128]);
    }
}
