//! Editor layout composition: tool rail, slide-out tool panel, inspector,
//! timeline, and the canvas in the middle. Owns the per-session editor
//! state, including the playback player and filmstrip thumbnails.

mod canvas;
pub mod export_dialog;
mod inspector;
mod timeline;
mod toolpanel;
mod toolrail;

use std::collections::HashMap;

use eframe::egui::{Context, Ui};

use crate::app::Session;
use crate::playback::Player;
use breez_core::events::{self, InputEvent, InputKind};
use timeline::clip::TrimDrag;
use timeline::filmstrip::Filmstrip;
use timeline::keyframes::ZoomDrag;
use timeline::waveform::{MusicDrag, Waveforms};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Media,
    Music,
}

impl Tool {
    fn title(self) -> &'static str {
        match self {
            Self::Media => "Media",
            Self::Music => "Music",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    OpenRecord,
}

/// Per-session editor state: playback, thumbnails, open panels, unsaved
/// project edits (`dirty`, saved by the app when the pointer is released).
pub struct EditorState {
    pub player: Player,
    pub dirty: bool,
    filmstrip: Filmstrip,
    /// Waveform peaks per music file, decoded on a worker thread.
    waveforms: Waveforms,
    trim: Option<TrimDrag>,
    zoom_drag: Option<ZoomDrag>,
    music_drag: Option<MusicDrag>,
    /// Index into `timeline.zoom` of the segment the inspector edits.
    selected_zoom: Option<usize>,
    /// Index into `timeline.music` of the selected track (Delete removes).
    selected_music: Option<usize>,
    /// Pending file-dialog result while the music import picker is open.
    music_picker: Option<std::sync::mpsc::Receiver<Vec<std::path::PathBuf>>>,
    /// Button-down events per take, sorted by time; drives click ripples,
    /// cursor-lane diamonds, and follow-cursor zoom anchors.
    clicks: HashMap<u32, Vec<InputEvent>>,
    resume_after_scrub: bool,
    open_tool: Option<Tool>,
    background_open: bool,
    zoom_open: bool,
    cursor_open: bool,
    audio_open: bool,
}

impl EditorState {
    pub fn new(ctx: &Context, session: &Session) -> Self {
        Self {
            player: Player::new(ctx.clone()),
            dirty: false,
            filmstrip: Filmstrip::spawn(ctx, session),
            waveforms: Waveforms::spawn(ctx, session),
            trim: None,
            zoom_drag: None,
            music_drag: None,
            selected_zoom: None,
            selected_music: None,
            music_picker: None,
            clicks: load_clicks(session),
            resume_after_scrub: false,
            open_tool: None,
            background_open: true,
            zoom_open: false,
            cursor_open: false,
            audio_open: false,
        }
    }
}

impl EditorState {
    /// Button-down events per take, for anything outside the editor that
    /// needs the same zoom anchors and ripples the preview uses.
    pub fn clicks(&self) -> &HashMap<u32, Vec<InputEvent>> {
        &self.clicks
    }
}

/// Click events per take from the package's event logs. Missing or
/// unreadable logs (Wayland records no events) just mean no clicks.
fn load_clicks(session: &Session) -> HashMap<u32, Vec<InputEvent>> {
    let mut clicks = HashMap::new();
    for take in &session.project.takes {
        let Some(events) = take
            .events
            .as_deref()
            .and_then(|rel| session.package.resolve(rel).ok())
            .and_then(|path| events::read_log(&path).ok())
        else {
            continue;
        };
        let downs: Vec<InputEvent> = events
            .into_iter()
            .filter(|e| e.kind == InputKind::Down)
            .collect();
        if !downs.is_empty() {
            clicks.insert(take.id, downs);
        }
    }
    clicks
}

pub fn show(ui: &mut Ui, state: &mut EditorState, session: &mut Session) -> Option<EditorAction> {
    state.player.tick(session);
    state
        .player
        .set_volume(session.project.style.system_audio_gain);
    state.dirty |= state.waveforms.poll(&mut session.project);
    toolpanel::poll_music_import(state, session);
    let action = toolrail::show(ui, &mut state.open_tool);
    timeline::show(ui, state, session);
    if let Some(tool) = state.open_tool {
        toolpanel::show(ui, tool, state, session);
    }
    inspector::show(ui, state, session);
    canvas::show(ui, state, session);
    action
}

/// `m:ss` display time from nanoseconds.
pub fn format_ns(ns: u64) -> String {
    crate::ui::record::format_secs(ns / 1_000_000_000)
}
