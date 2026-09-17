//! Composite one synthetic frame to a PNG, so the layer stack can be
//! eyeballed without running the app.
//!
//! cargo run -p breez-render --example compose -- out.png

use breez_core::project::Style;
use breez_core::render::ZoomView;
use breez_render::{Compositor, FrameParams, SourceFrame};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dest = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "compose.png".to_owned());
    let (src_w, src_h) = (1280u32, 720u32);
    let mut source = Vec::with_capacity((src_w * src_h * 4) as usize);
    for y in 0..src_h {
        for x in 0..src_w {
            let value = if ((x / 80) + (y / 80)).is_multiple_of(2) {
                0xe0
            } else {
                0x20
            };
            source.extend_from_slice(&[value, value, value, 0xff]);
        }
    }

    let style = Style::default();
    let (out_w, out_h) = (1920u32, 1080u32);
    let mut compositor = Compositor::new(out_w, out_h);
    // Compose repeatedly: the first call pays for the rayon pool spin-up, so
    // only the steady-state average says anything about export throughput.
    const RUNS: u32 = 60;
    let mut started = std::time::Instant::now();
    for run in 0..RUNS {
        if run == 1 {
            started = std::time::Instant::now();
        }
        compositor.compose(
            SourceFrame {
                data: &source,
                width: src_w,
                height: src_h,
            },
            &FrameParams {
                style: &style,
                zoom: ZoomView::NEUTRAL,
                ripples: &[],
                click_highlight: false,
            },
        );
    }
    let elapsed = started.elapsed() / (RUNS - 1);
    let composed = compositor.compose(
        SourceFrame {
            data: &source,
            width: src_w,
            height: src_h,
        },
        &FrameParams {
            style: &style,
            zoom: ZoomView::NEUTRAL,
            ripples: &[],
            click_highlight: false,
        },
    );
    image::save_buffer(&dest, composed, out_w, out_h, image::ColorType::Rgba8)?;
    println!("wrote {dest} ({out_w}x{out_h}); {elapsed:?} per frame");
    Ok(())
}
