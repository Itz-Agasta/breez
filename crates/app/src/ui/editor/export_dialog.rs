//! Export dialog: resolution preset, quality and destination, then progress
//! with a cancel button.
//!
//! Owns the `ExportJob` while it runs; the UI thread only polls it. The
//! destination picker blocks, so it runs on a throwaway thread and reports
//! back over a one-shot channel, the same way the music import does.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui::{Context, Modal, ProgressBar, Ui};

use crate::app::Session;
use crate::export::{ExportJob, Preset, Progress, Quality, Settings};
use crate::theme;
use crate::ui::widgets::{self, segmented::Segment};
use breez_core::events::InputEvent;
use breez_core::layout;

const PRESETS: [Preset; 3] = [Preset::P1080, Preset::P1440, Preset::Source];
const QUALITIES: [Quality; 3] = [Quality::High, Quality::Balanced, Quality::Small];

#[derive(Default)]
pub struct State {
    pub open: bool,
    preset: usize,
    quality: usize,
    dest: Option<PathBuf>,
    picker: Option<mpsc::Receiver<Option<PathBuf>>>,
    job: Option<ExportJob>,
    /// Outcome of the last finished export, shown until dismissed.
    result: Option<Result<PathBuf, String>>,
}

impl State {
    /// Open the dialog, clearing whatever the previous export left behind.
    pub fn open(&mut self) {
        self.open = true;
        self.result = None;
    }

    pub fn show(
        &mut self,
        ctx: &Context,
        session: &Session,
        clicks: &HashMap<u32, Vec<InputEvent>>,
    ) {
        self.poll_picker();
        if !self.open {
            return;
        }
        let mut modal = Modal::new("export".into());
        modal = modal.frame(
            eframe::egui::Frame::new()
                .fill(theme::BG_PANEL)
                .inner_margin(20.0)
                .corner_radius(theme::RADIUS_CARD),
        );
        modal.show(ctx, |ui| {
            ui.set_width(380.0);
            match self.job.is_some() {
                true => self.progress_view(ui),
                false => self.settings_view(ui, session, clicks),
            }
        });
    }

    fn settings_view(
        &mut self,
        ui: &mut Ui,
        session: &Session,
        clicks: &HashMap<u32, Vec<InputEvent>>,
    ) {
        ui.label(
            eframe::egui::RichText::new("Export")
                .font(eframe::egui::FontId::new(15.0, theme::semibold()))
                .color(theme::TEXT),
        );
        ui.add_space(14.0);

        ui.label(eframe::egui::RichText::new("Resolution").color(theme::TEXT_MUTED));
        ui.add_space(6.0);
        let labels: Vec<Segment> = PRESETS.iter().map(|p| Segment::new(p.label())).collect();
        widgets::segmented::segmented(ui, &mut self.preset, &labels, 28.0);

        // The take export will actually render, not takes.first(): a package
        // can hold takes the timeline never uses.
        let take_height = session
            .project
            .primary_take()
            .map_or(1080, |take| take.height);
        let (width, height) = layout::output_size(
            &session.project.style.ratio,
            PRESETS[self.preset].short_side(take_height),
        );
        ui.add_space(6.0);
        ui.label(
            eframe::egui::RichText::new(format!("{width} x {height}")).color(theme::TEXT_MUTED),
        );
        ui.add_space(14.0);

        ui.label(eframe::egui::RichText::new("Quality").color(theme::TEXT_MUTED));
        ui.add_space(6.0);
        let labels: Vec<Segment> = QUALITIES.iter().map(|q| Segment::new(q.label())).collect();
        widgets::segmented::segmented(ui, &mut self.quality, &labels, 28.0);
        ui.add_space(14.0);

        ui.label(eframe::egui::RichText::new("Destination").color(theme::TEXT_MUTED));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if widgets::button::ghost(ui, "Choose\u{2026}", true).clicked() {
                self.open_picker(ui.ctx(), session);
            }
            let name = self
                .dest
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "no file chosen".to_owned());
            ui.label(eframe::egui::RichText::new(name).color(theme::TEXT_MUTED));
        });

        if let Some(Err(message)) = &self.result {
            ui.add_space(10.0);
            ui.label(eframe::egui::RichText::new(message.as_str()).color(theme::RECORD_RED));
        }
        if let Some(Ok(path)) = &self.result {
            ui.add_space(10.0);
            ui.label(
                eframe::egui::RichText::new(format!("Wrote {}", path.display()))
                    .color(theme::TEXT_MUTED),
            );
        }

        ui.add_space(18.0);
        ui.horizontal(|ui| {
            if widgets::button::primary(ui, "Export", self.dest.is_some()).clicked()
                && let Some(dest) = self.dest.clone()
            {
                self.result = None;
                self.job = Some(ExportJob::spawn(
                    &session.package,
                    &session.project,
                    clicks,
                    Settings {
                        preset: PRESETS[self.preset],
                        quality: QUALITIES[self.quality],
                        dest,
                    },
                ));
            }
            if widgets::button::ghost(ui, "Close", true).clicked() {
                self.open = false;
            }
        });
    }

    fn progress_view(&mut self, ui: &mut Ui) {
        let Some(job) = &mut self.job else {
            return;
        };
        let Progress {
            done,
            total,
            finished,
        } = job.poll();

        if let Some(outcome) = finished {
            self.job = None;
            self.result = Some(outcome);
            return;
        }

        ui.label(
            eframe::egui::RichText::new("Exporting")
                .font(eframe::egui::FontId::new(15.0, theme::semibold()))
                .color(theme::TEXT),
        );
        ui.add_space(14.0);
        let fraction = if total == 0 {
            0.0
        } else {
            done as f32 / total as f32
        };
        ui.add(ProgressBar::new(fraction).desired_height(8.0));
        ui.add_space(8.0);
        ui.label(
            eframe::egui::RichText::new(format!("frame {done} of {total}"))
                .color(theme::TEXT_MUTED),
        );
        ui.add_space(18.0);
        if widgets::button::ghost(ui, "Cancel", true).clicked() {
            job.cancel();
        }
        // An export produces no input events, so egui would otherwise idle
        // and the bar would freeze until the pointer moved.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }

    /// Open the (blocking) save dialog on its own thread; at most one at a
    /// time.
    fn open_picker(&mut self, ctx: &Context, session: &Session) {
        if self.picker.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let name = format!("{}.mp4", session.project.name);
        let dir = session
            .package
            .root()
            .parent()
            .map(std::path::Path::to_path_buf);
        let spawned = std::thread::Builder::new()
            .name("breez-exportpicker".to_owned())
            .spawn(move || {
                let mut dialog = rfd::FileDialog::new()
                    .set_title("Export video")
                    .set_file_name(name)
                    .add_filter("MP4", &["mp4"]);
                if let Some(dir) = dir {
                    dialog = dialog.set_directory(dir);
                }
                let _ = tx.send(dialog.save_file());
                // The UI may be idle while the dialog was up; wake it so the
                // chosen path lands immediately.
                ctx.request_repaint();
            });
        if spawned.is_ok() {
            self.picker = Some(rx);
        }
    }

    /// Collect the picked destination. Called every frame so it lands even if
    /// the dialog outlived the modal.
    fn poll_picker(&mut self) {
        let Some(rx) = &self.picker else {
            return;
        };
        let picked = match rx.try_recv() {
            Ok(picked) => picked,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => None,
        };
        self.picker = None;
        if let Some(mut path) = picked {
            // The portal's "All files" option can bypass the dialog filter.
            if path.extension().is_none() {
                path.set_extension("mp4");
            }
            self.dest = Some(path);
        }
    }
}
