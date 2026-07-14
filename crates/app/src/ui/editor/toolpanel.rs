//! 302px slide-out tool panel. Music ships click-to-browse import and the
//! track list (Phase 5); Media still stubs until the open-package flow.
//!
//! The file dialog (rfd, portal-backed) blocks, so it runs on a throwaway
//! thread; the picked paths come back over a one-shot channel the editor
//! polls every frame.

use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui::{
    Align, Align2, CornerRadius, FontFamily, FontId, Frame, Layout, Panel, RichText, Sense, Stroke,
    StrokeKind, Ui, vec2,
};

use super::{EditorState, Tool, format_ns};
use crate::app::Session;
use crate::theme;

/// Import filter: what rodio's bundled symphonia decoders handle.
const AUDIO_EXTS: &[&str] = &["mp3", "m4a", "aac", "wav", "flac", "ogg"];

pub fn show(ui: &mut Ui, tool: Tool, state: &mut EditorState, session: &mut Session) {
    Panel::left("toolpanel")
        .exact_size(theme::TOOLPANEL_WIDTH)
        .frame(Frame::new().fill(theme::BG_PANEL).inner_margin(16))
        .show_separator_line(false)
        .show(ui, |ui| {
            let panel = ui.max_rect();
            ui.painter().vline(
                panel.max.x + 15.5,
                panel.y_range().expand(16.0),
                Stroke::new(1.0, theme::BORDER),
            );
            ui.label(
                RichText::new(tool.title())
                    .font(FontId::new(13.0, theme::semibold()))
                    .color(theme::TEXT),
            );
            ui.add_space(14.0);
            match tool {
                Tool::Media => {
                    muted(ui, "No imports yet.");
                    ui.add_space(6.0);
                    faint(ui, "Recorded takes appear here; file import comes later.");
                }
                Tool::Music => music_panel(ui, state, session),
            }
        });
}

fn music_panel(ui: &mut Ui, state: &mut EditorState, session: &mut Session) {
    if session.project.timeline.music.is_empty() {
        muted(ui, "No music tracks.");
        ui.add_space(10.0);
    } else {
        let mut remove = None;
        for (index, track) in session.project.timeline.music.iter().enumerate() {
            if track_row(ui, track, state.selected_music == Some(index)) {
                remove = Some(index);
            }
            ui.add_space(6.0);
        }
        if let Some(index) = remove {
            session.project.timeline.music.remove(index);
            state.selected_music = None;
            state.dirty = true;
        }
        ui.add_space(6.0);
    }
    if browse_zone(ui, state.music_picker.is_some()) {
        open_music_picker(ui.ctx(), state);
    }
}

/// One track card: name + duration, an ✕ on the right. Returns true when
/// the track should be removed.
fn track_row(ui: &mut Ui, track: &breez_core::project::MusicTrack, selected: bool) -> bool {
    let mut remove = false;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(theme::RADIUS_BUTTON),
        theme::BG_CONTROL,
    );
    if selected {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(theme::RADIUS_BUTTON),
            Stroke::new(1.0, theme::ACCENT),
            StrokeKind::Inside,
        );
    }
    let mut content = ui.new_child(
        eframe::egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 6.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    content.vertical(|ui| {
        ui.add_space(2.0);
        ui.label(
            RichText::new(track_name(&track.file))
                .font(FontId::new(12.0, theme::medium()))
                .color(theme::TEXT),
        );
        ui.label(
            RichText::new(if track.duration_ns > 0 {
                format_ns(track.duration_ns)
            } else {
                "\u{2026}".to_owned()
            })
            .font(FontId::new(10.5, FontFamily::Monospace))
            .color(theme::TEXT_MUTED),
        );
    });
    content.with_layout(Layout::right_to_left(Align::Center), |ui| {
        remove = crate::ui::widgets::button::icon(ui, "\u{2715}", true).clicked();
    });
    remove
}

fn track_name(rel: &str) -> String {
    std::path::Path::new(rel)
        .file_stem()
        .map_or_else(|| rel.to_owned(), |s| s.to_string_lossy().into_owned())
}

/// The dashed import card. Returns true on click; renders busy while a
/// dialog is already open.
fn browse_zone(ui: &mut Ui, picking: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), 84.0),
        if picking {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
    let hovered = !picking && response.hovered();
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::RADIUS_CARD),
        Stroke::new(
            1.0,
            if hovered {
                theme::MUSIC_BLUE
            } else {
                theme::BORDER_STRONG
            },
        ),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        if picking {
            "Choosing\u{2026}"
        } else {
            "Browse audio files\u{2026}"
        },
        FontId::new(11.5, FontFamily::Proportional),
        if hovered {
            theme::TEXT_MUTED
        } else {
            theme::TEXT_FAINT
        },
    );
    response.clicked()
}

/// Open the (blocking) file dialog on its own thread; at most one at a time.
fn open_music_picker(ctx: &eframe::egui::Context, state: &mut EditorState) {
    if state.music_picker.is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("breez-filepicker".to_owned())
        .spawn(move || {
            let files = rfd::FileDialog::new()
                .set_title("Import music")
                .add_filter("Audio", AUDIO_EXTS)
                .pick_files()
                .unwrap_or_default();
            let _ = tx.send(files);
            // The UI may be idle while the dialog was up; wake it so the
            // import lands immediately.
            ctx.request_repaint();
        })
        .expect("spawn file picker thread");
    state.music_picker = Some(rx);
}

/// Collect files picked in the dialog and add them as music tracks. Called
/// every editor frame so an import lands even if the panel was closed.
pub fn poll_music_import(state: &mut EditorState, session: &mut Session) {
    let Some(rx) = &state.music_picker else {
        return;
    };
    let files: Vec<PathBuf> = match rx.try_recv() {
        Ok(files) => files,
        Err(mpsc::TryRecvError::Empty) => return,
        Err(mpsc::TryRecvError::Disconnected) => Vec::new(),
    };
    state.music_picker = None;
    for path in files {
        // The portal's "All files" option can bypass the dialog filter.
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
        if !ext.is_some_and(|e| AUDIO_EXTS.contains(&e.as_str())) {
            log::warn!("import {}: not an audio file", path.display());
            continue;
        }
        match session.package.import_music(&path) {
            Ok(rel) => {
                state.waveforms.request(&session.package, &rel);
                session
                    .project
                    .timeline
                    .music
                    .push(breez_core::project::MusicTrack {
                        file: rel,
                        offset_ns: 0,
                        gain: 0.5,
                        fade_in_ns: 500_000_000,
                        fade_out_ns: 1_000_000_000,
                        duration_ns: 0,
                    });
                state.dirty = true;
            }
            Err(e) => log::warn!("import {}: {e}", path.display()),
        }
    }
}

fn muted(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(FontId::new(12.5, FontFamily::Proportional))
            .color(theme::TEXT_MUTED),
    );
}

fn faint(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(FontId::new(11.5, FontFamily::Proportional))
            .color(theme::TEXT_FAINT),
    );
}
