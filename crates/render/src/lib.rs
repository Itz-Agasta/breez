//! breez-render: frame compositor.
//!
//! Turns a source frame + `RenderParams` (style, zoom curve, ripples) into a
//! composited output frame for export. The egui preview draws the same
//! `RenderParams` with painter primitives; a post-MVP wgpu pass will unify
//! both paths. CPU (rayon) implementation for v1.
