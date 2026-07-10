//! Editor layout composition: tool rail, slide-out tool panel, inspector,
//! timeline chrome, and the canvas in the middle. Phase 2 ships the chrome;
//! playback and real timeline data arrive in Phase 3.

mod canvas;
mod inspector;
mod timeline;
mod toolpanel;
mod toolrail;

use eframe::egui::Ui;

use crate::app::Session;
use breez_core::project::ZoomSegment;

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

/// Per-session editor UI state (open panels, section folds, static drafts).
pub struct EditorState {
    open_tool: Option<Tool>,
    background_open: bool,
    zoom_open: bool,
    cursor_open: bool,
    audio_open: bool,
    /// Placeholder values for the static zoom section; real zoom segments
    /// bind here in Phase 4.
    zoom_draft: ZoomSegment,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            open_tool: None,
            background_open: true,
            zoom_open: false,
            cursor_open: false,
            audio_open: false,
            zoom_draft: ZoomSegment {
                in_ns: 0,
                out_ns: 0,
                level: 1.8,
                anchor: [0.5, 0.5],
                follow_cursor: true,
                easing: breez_core::project::Easing::Smooth,
            },
        }
    }
}

pub fn show(ui: &mut Ui, state: &mut EditorState, session: &mut Session) -> Option<EditorAction> {
    let action = toolrail::show(ui, &mut state.open_tool);
    timeline::show(ui, session);
    if let Some(tool) = state.open_tool {
        toolpanel::show(ui, tool);
    }
    inspector::show(ui, state, session);
    canvas::show(ui, session);
    action
}

/// `m:ss` display time from nanoseconds.
pub fn format_ns(ns: u64) -> String {
    crate::ui::record::format_secs(ns / 1_000_000_000)
}
