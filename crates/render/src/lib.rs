//! breez-render: frame compositor.
//!
//! Turns a source frame plus the project style, zoom curve and ripples into
//! a composited output frame for export. The egui preview draws the same
//! `breez_core::layout` geometry with painter primitives; a post-MVP wgpu
//! pass will unify both paths. CPU (rayon) implementation for v1.

mod compose;
mod raster;

pub use compose::{Compositor, FrameParams, SourceFrame};
