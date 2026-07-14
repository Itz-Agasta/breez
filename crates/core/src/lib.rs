//! breez-core: the project model.
//!
//! Owns the `.rec` package layout, the versioned `project.json` timeline,
//! and the input-event log. Everything here is plain data + disk IO; no
//! capture, codec, or UI dependencies.

pub mod events;
pub mod package;
pub mod project;
pub mod render;
pub mod timeline;
