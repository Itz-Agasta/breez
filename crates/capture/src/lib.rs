//! breez-capture: recording pipeline.
//!
//! Drives a pinray capture session (display video + system audio), feeds the
//! frames into breez-codec encoders, and appends input events to the package
//! event log. Produces a valid `.rec` package even if the process dies
//! mid-recording (fragmented MP4 + append-only logs).

mod input;
pub mod recorder;

pub use recorder::{CaptureError, RecordConfig, Recorder, TakeSummary};
