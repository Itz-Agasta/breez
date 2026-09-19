//! Filmstrip thumbnails for the screen lane: a worker thread generates tiny
//! JPEGs into the package cache (skipped when already present), the UI loads
//! them into textures lazily as clip slots ask for them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};

use crate::app::Session;

const THUMB_WIDTH: u32 = 160;

pub struct Filmstrip {
    rx: mpsc::Receiver<(u32, Vec<PathBuf>)>,
    thumbs: HashMap<u32, Vec<PathBuf>>,
    /// `None` marks a jpeg that failed to load, so it is not retried.
    textures: HashMap<(u32, usize), Option<TextureHandle>>,
}

impl Filmstrip {
    /// Kick off thumbnail generation for every take in the session.
    pub fn spawn(ctx: &Context, session: &Session) -> Self {
        let (tx, rx) = mpsc::channel();
        let jobs: Vec<(u32, PathBuf, PathBuf, u32, u64)> = session
            .project
            .takes
            .iter()
            .filter_map(|take| {
                let video = session.package.resolve(&take.video).ok()?;
                let out_dir = session.package.thumbs_dir(take.id);
                Some((
                    take.id,
                    video,
                    out_dir,
                    thumb_count(take.duration_ns),
                    take.duration_ns,
                ))
            })
            .collect();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("breez-thumbs".to_owned())
            .spawn(move || {
                for (id, video, out_dir, count, duration_ns) in jobs {
                    match breez_codec::generate_thumbs(
                        &video,
                        &out_dir,
                        count,
                        THUMB_WIDTH,
                        duration_ns,
                    ) {
                        Ok(paths) => {
                            if tx.send((id, paths)).is_err() {
                                return;
                            }
                            ctx.request_repaint();
                        }
                        Err(e) => log::warn!("thumbs for take {id}: {e}"),
                    }
                }
            })
            .expect("spawn thumbs thread");
        Self {
            rx,
            thumbs: HashMap::new(),
            textures: HashMap::new(),
        }
    }

    /// Drain finished takes from the worker. Call once per frame.
    pub fn poll(&mut self) {
        while let Ok((take, paths)) = self.rx.try_recv() {
            self.thumbs.insert(take, paths);
        }
    }

    /// Texture for the thumb nearest `fraction` (0..1) of the take's
    /// duration, loading the jpeg on first use.
    pub fn texture(&mut self, ctx: &Context, take: u32, fraction: f32) -> Option<&TextureHandle> {
        let paths = self.thumbs.get(&take)?;
        if paths.is_empty() {
            return None;
        }
        let index = (fraction.clamp(0.0, 1.0) * (paths.len() - 1) as f32).round() as usize;
        let slot = self
            .textures
            .entry((take, index))
            .or_insert_with(|| load_texture(ctx, &paths[index]));
        slot.as_ref()
    }
}

fn load_texture(ctx: &Context, path: &std::path::Path) -> Option<TextureHandle> {
    let image = match image::open(path) {
        Ok(image) => image.to_rgba8(),
        Err(e) => {
            log::warn!("thumb {}: {e}", path.display());
            return None;
        }
    };
    let size = [image.width() as usize, image.height() as usize];
    let color = ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture(
        format!("thumb-{}", path.display()),
        color,
        TextureOptions::LINEAR,
    ))
}

/// One thumb every ~2s, bounded so short takes still get a strip and long
/// ones stay cheap.
fn thumb_count(duration_ns: u64) -> u32 {
    (duration_ns / 2_000_000_000).clamp(8, 60) as u32
}
