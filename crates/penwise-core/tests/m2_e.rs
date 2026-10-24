//! Owner: slice e (M2).
//!
//! Undo/redo over a multi-writer scene (`edit::undo`) and the `PWIT`/`PWED` item and edit blobs
//! (`codec::{item, edit}`), tested through the public API only.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test harness: failures are reported by panicking"
)]

#[path = "m2_e/edit_codec.rs"]
mod edit_codec;
#[path = "m2_e/fixtures.rs"]
mod fixtures;
#[path = "m2_e/item_codec.rs"]
mod item_codec;
#[path = "m2_e/undo.rs"]
mod undo;
