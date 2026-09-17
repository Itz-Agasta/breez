//! Root application state: mode switching, the record flow state machine,
//! and per-frame layout dispatch. Recording runs on the capture thread;
//! stopping joins it on a helper thread so the UI never blocks.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use breez_capture::{CaptureError, RecordConfig, Recorder, TakeSummary};
use breez_core::package::RecPackage;
use breez_core::project::Project;
use eframe::egui;

use crate::theme;
use crate::ui::editor::export_dialog;
use crate::ui::editor::{self, EditorAction, EditorState};
use crate::ui::record::{self, RecordAction, RecordState};
use crate::ui::titlebar::{self, TitlebarAction, TitlebarState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Record,
    Edit,
}

/// An open `.rec` package and its loaded project model.
pub struct Session {
    pub package: RecPackage,
    pub project: Project,
}

enum RecordFlow {
    Idle,
    Recording {
        recorder: Recorder,
        package_root: PathBuf,
        started: Instant,
    },
    /// Waiting for the capture thread to join and the take to be written.
    Stopping {
        package_root: PathBuf,
        rx: mpsc::Receiver<Result<TakeSummary, CaptureError>>,
    },
}

pub struct BreezApp {
    mode: Mode,
    flow: RecordFlow,
    session: Option<Session>,
    editor: Option<EditorState>,
    export: export_dialog::State,
    error: Option<String>,
}

impl BreezApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        Self {
            mode: Mode::Record,
            flow: RecordFlow::Idle,
            session: None,
            editor: None,
            export: export_dialog::State::default(),
            error: None,
        }
    }

    /// Drive the record flow between frames: keep repainting for the timer,
    /// notice a capture thread that ended on its own (portal cancel, backend
    /// error), and collect the finished take.
    fn poll_flow(&mut self, ctx: &egui::Context) {
        match &self.flow {
            RecordFlow::Recording { recorder, .. } => {
                ctx.request_repaint_after(Duration::from_millis(100));
                if recorder.is_finished() {
                    self.finish_recording();
                }
            }
            RecordFlow::Stopping { rx, .. } => {
                ctx.request_repaint_after(Duration::from_millis(100));
                if let Ok(result) = rx.try_recv() {
                    let flow = std::mem::replace(&mut self.flow, RecordFlow::Idle);
                    if let RecordFlow::Stopping { package_root, .. } = flow {
                        self.apply_take(ctx, package_root, result);
                    }
                }
            }
            RecordFlow::Idle => {}
        }
    }

    fn start_recording(&mut self) {
        let root = match &self.session {
            // Recording again appends a new take to the open package.
            Some(session) => session.package.root().to_path_buf(),
            None => match next_package_path() {
                Ok(path) => path,
                Err(e) => {
                    self.error = Some(e);
                    return;
                }
            },
        };
        match Recorder::start(&root, RecordConfig::default()) {
            Ok(recorder) => {
                self.error = None;
                self.flow = RecordFlow::Recording {
                    recorder,
                    package_root: root,
                    started: Instant::now(),
                };
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Move the recorder to a helper thread to join the capture thread;
    /// `poll_flow` picks up the result.
    fn finish_recording(&mut self) {
        let flow = std::mem::replace(&mut self.flow, RecordFlow::Idle);
        if let RecordFlow::Recording {
            recorder,
            package_root,
            ..
        } = flow
        {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(recorder.stop());
            });
            self.flow = RecordFlow::Stopping { package_root, rx };
        }
    }

    fn apply_take(
        &mut self,
        ctx: &egui::Context,
        package_root: PathBuf,
        result: Result<TakeSummary, CaptureError>,
    ) {
        let opened = result.and_then(|summary| {
            let package = RecPackage::open(&package_root)?;
            let project = Project::load(&package)?;
            Ok((summary, package, project))
        });
        match opened {
            Ok((_, package, project)) => {
                let session = Session { package, project };
                self.editor = Some(EditorState::new(ctx, &session));
                self.session = Some(session);
                self.error = None;
                self.mode = Mode::Edit;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Persist pending project edits once the pointer is up, so slider drags
    /// don't write (and fsync) every frame.
    fn autosave(&mut self, ctx: &egui::Context) {
        let (Some(editor), Some(session)) = (&mut self.editor, &self.session) else {
            return;
        };
        if editor.dirty && !ctx.input(|i| i.pointer.any_down()) {
            editor.dirty = false;
            if let Err(e) = session.project.save(&session.package) {
                log::error!("save project: {e}");
                self.error = Some(format!("save project: {e}"));
            }
        }
    }
}

impl eframe::App for BreezApp {
    /// Finish an in-flight recording cleanly instead of leaving it to crash
    /// recovery when the window closes mid-take.
    fn on_exit(&mut self) {
        match std::mem::replace(&mut self.flow, RecordFlow::Idle) {
            RecordFlow::Recording { recorder, .. } => {
                let _ = recorder.stop();
            }
            RecordFlow::Stopping { rx, .. } => {
                let _ = rx.recv();
            }
            RecordFlow::Idle => {}
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_flow(ui.ctx());

        let name = self.session.as_ref().map(|s| s.project.name.clone());
        let titlebar_state = TitlebarState {
            mode: self.mode,
            project_name: name.as_deref(),
            can_edit: self.session.is_some(),
            busy: !matches!(self.flow, RecordFlow::Idle),
        };
        match titlebar::show(ui, &titlebar_state) {
            Some(TitlebarAction::SetMode(mode)) => {
                if mode == Mode::Record
                    && let Some(editor) = &mut self.editor
                {
                    editor.player.pause();
                }
                self.mode = mode;
            }
            Some(TitlebarAction::Export) => {
                if let Some(editor) = &mut self.editor {
                    editor.player.pause();
                }
                self.export.open();
            }
            None => {}
        }

        match self.mode {
            Mode::Record => {
                let state = match &self.flow {
                    RecordFlow::Idle => RecordState::Idle,
                    RecordFlow::Recording { started, .. } => RecordState::Recording {
                        elapsed: started.elapsed(),
                    },
                    RecordFlow::Stopping { .. } => RecordState::Saving,
                };
                match record::show(ui, &state, self.error.as_deref()) {
                    Some(RecordAction::Start) => self.start_recording(),
                    Some(RecordAction::Stop) => self.finish_recording(),
                    None => {}
                }
            }
            Mode::Edit => match (self.session.as_mut(), self.editor.as_mut()) {
                (Some(session), Some(editor)) => {
                    if let Some(EditorAction::OpenRecord) = editor::show(ui, editor, session) {
                        editor.player.pause();
                        self.mode = Mode::Record;
                    }
                }
                _ => self.mode = Mode::Record,
            },
        }
        if let (Some(session), Some(editor)) = (&self.session, &self.editor) {
            self.export.show(ui.ctx(), session, editor.clicks());
        }
        self.autosave(ui.ctx());
    }
}

/// Next free `~/Videos/Breez/Untitled-NNN.rec`. Linux-first; platform video
/// dirs come with the packaging pass.
fn next_package_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    let dir = PathBuf::from(home).join("Videos").join("Breez");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    (1..1000)
        .map(|n| dir.join(format!("Untitled-{n:03}.rec")))
        .find(|p| !p.exists())
        .ok_or_else(|| "too many untitled packages".to_owned())
}
