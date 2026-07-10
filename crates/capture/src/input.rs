//! Global input listener feeding the take's event log.
//!
//! Uses rdev (X11/Windows/macOS). Wayland has no global listener; there we
//! skip event logging entirely and say so once. Timestamps are anchored to
//! the first video frame so events line up with `stream_time_ns`-derived
//! take time; events arriving before that anchor are dropped.
//!
//! rdev's `listen` blocks its thread forever with no shutdown API, so one
//! detached listener thread is started for the whole process; recordings
//! install and remove the active log it writes to.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use breez_core::events::{EventLogWriter, InputEvent, InputKind, MouseButton};

struct ActiveLog {
    writer: EventLogWriter,
    anchor: Arc<OnceLock<Instant>>,
    display_size: Arc<OnceLock<(u32, u32)>>,
    /// Unknown until the first motion event; button events before that are
    /// dropped rather than logged at a fake (0, 0).
    last_pos: Option<(f32, f32)>,
    write_failed: bool,
}

static ACTIVE: Mutex<Option<ActiveLog>> = Mutex::new(None);
static LISTENER: OnceLock<()> = OnceLock::new();

pub(crate) struct InputLogger;

impl InputLogger {
    /// Start logging, or return `None` where global listening is not
    /// possible (Wayland) or the log file cannot be created.
    pub(crate) fn start(
        path: PathBuf,
        anchor: Arc<OnceLock<Instant>>,
        display_size: Arc<OnceLock<(u32, u32)>>,
    ) -> Option<Self> {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            log::warn!("Wayland session: no global input listener, skipping event log");
            return None;
        }
        let writer = match EventLogWriter::create(&path) {
            Ok(w) => w,
            Err(e) => {
                log::warn!("cannot create event log {path:?}: {e}");
                return None;
            }
        };
        if let Ok(mut guard) = ACTIVE.lock() {
            *guard = Some(ActiveLog {
                writer,
                anchor,
                display_size,
                last_pos: None,
                write_failed: false,
            });
        }
        ensure_listener();
        Some(Self)
    }

    /// Stop recording events and flush the log. The shared listener thread
    /// keeps running (rdev has no shutdown API) but writes nothing further.
    pub(crate) fn stop(&self) {
        if let Ok(mut guard) = ACTIVE.lock()
            && let Some(mut active) = guard.take()
        {
            let _ = active.writer.flush();
        }
    }
}

/// Spawn the process-wide rdev listener on first use.
fn ensure_listener() {
    LISTENER.get_or_init(|| {
        std::thread::spawn(|| {
            let result = rdev::listen(|event| {
                if let Ok(mut guard) = ACTIVE.lock()
                    && let Some(active) = guard.as_mut()
                {
                    handle(active, &event);
                }
            });
            if let Err(e) = result {
                log::warn!("input listener unavailable: {e:?}");
            }
        });
    });
}

fn handle(active: &mut ActiveLog, event: &rdev::Event) {
    let Some(start) = active.anchor.get() else {
        return;
    };
    let Some(&(w, h)) = active.display_size.get() else {
        return;
    };
    let (kind, button, pos) = match event.event_type {
        rdev::EventType::MouseMove { x, y } => {
            let pos = (x as f32 / w as f32, y as f32 / h as f32);
            active.last_pos = Some(pos);
            (InputKind::Move, None, pos)
        }
        rdev::EventType::ButtonPress(b) => {
            let Some(pos) = active.last_pos else { return };
            (InputKind::Down, map_button(b), pos)
        }
        rdev::EventType::ButtonRelease(b) => {
            let Some(pos) = active.last_pos else { return };
            (InputKind::Up, map_button(b), pos)
        }
        _ => return,
    };
    let record = InputEvent {
        t_ns: start.elapsed().as_nanos() as u64,
        kind,
        button,
        x: pos.0.clamp(0.0, 1.0),
        y: pos.1.clamp(0.0, 1.0),
    };
    // Flush per event: the JSONL crash-safety promise needs lines at the OS
    // before a kill -9, and input rates are far too low for this to matter.
    let result = active
        .writer
        .write(&record)
        .and_then(|()| active.writer.flush());
    if let Err(e) = result
        && !active.write_failed
    {
        active.write_failed = true;
        log::warn!("event log write failed, log will be incomplete: {e}");
    }
}

fn map_button(b: rdev::Button) -> Option<MouseButton> {
    match b {
        rdev::Button::Left => Some(MouseButton::Left),
        rdev::Button::Right => Some(MouseButton::Right),
        rdev::Button::Middle => Some(MouseButton::Middle),
        rdev::Button::Unknown(_) => None,
    }
}
