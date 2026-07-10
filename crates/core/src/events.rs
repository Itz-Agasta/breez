//! Append-only input-event log: one JSON object per line, one file per take.
//! JSONL is crash-safe by construction; a torn final line is skipped on read.

use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::package::PackageError;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct InputEvent {
    /// Nanoseconds since the first video frame of the take.
    pub t_ns: u64,
    pub kind: InputKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub button: Option<MouseButton>,
    /// Normalized 0..1 position within the captured display.
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputKind {
    Move,
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

pub struct EventLogWriter {
    out: BufWriter<fs::File>,
}

impl EventLogWriter {
    pub fn create(path: &Path) -> Result<Self, PackageError> {
        Ok(Self {
            out: BufWriter::new(fs::File::create(path)?),
        })
    }

    pub fn write(&mut self, event: &InputEvent) -> Result<(), PackageError> {
        serde_json::to_writer(&mut self.out, event)?;
        self.out.write_all(b"\n")?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), PackageError> {
        self.out.flush()?;
        Ok(())
    }
}

/// Read a log back, silently dropping unparseable lines (a crash can tear
/// the last one).
pub fn read_log(path: &Path) -> Result<Vec<InputEvent>, PackageError> {
    let reader = BufReader::new(fs::File::open(path)?);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if let Ok(event) = serde_json::from_str::<InputEvent>(&line) {
            events.push(event);
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(t_ns: u64) -> InputEvent {
        InputEvent {
            t_ns,
            kind: InputKind::Down,
            button: Some(MouseButton::Left),
            x: 0.5,
            y: 0.25,
        }
    }

    #[test]
    fn written_events_read_back_identically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.jsonl");
        let mut writer = EventLogWriter::create(&path).unwrap();
        writer.write(&click(100)).unwrap();
        writer.write(&click(200)).unwrap();
        writer.flush().unwrap();
        assert_eq!(read_log(&path).unwrap(), vec![click(100), click(200)]);
    }

    #[test]
    fn read_log_skips_torn_final_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.jsonl");
        let mut writer = EventLogWriter::create(&path).unwrap();
        writer.write(&click(100)).unwrap();
        writer.flush().unwrap();
        use std::io::Write as _;
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"t_ns\":200,\"ki").unwrap();
        assert_eq!(read_log(&path).unwrap(), vec![click(100)]);
    }
}
