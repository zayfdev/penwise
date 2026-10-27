//! Browser bindings for the ink engine: the replica viewer (frames in, draw lists out), the author side of
//! browser drawing (pointer samples in, live deltas and ops out).
//!
//! No geometry or sync rule lives here; this only converts between JS values and [`penwise_core`] /
//! [`penwise_proto`].

pub mod decode;
pub mod draw_list;
pub mod encode;
pub mod error;
pub mod pointer;
pub mod received;
pub mod viewer;

use wasm_bindgen::prelude::wasm_bindgen;

/// Names of the brush presets in `Preset::ALL` order (the `preset` argument of `Viewer.stroke_begin`).
#[wasm_bindgen]
#[must_use]
pub fn preset_names() -> Vec<String> {
    penwise_core::brush::Preset::ALL
        .iter()
        .map(|p| p.name().to_owned())
        .collect()
}
