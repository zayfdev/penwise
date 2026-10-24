//! Browser bindings for the Penwise viewer: `penwise-proto` replica + live strokes and `penwise-core` geometry,
//! exposed through `wasm-bindgen` (built by `wasm-pack build --target web crates/penwise-wasm`).
//!
//! Owner: contract. Only the contract owner edits this file; the M3 contract lives in
//! `docs/contracts/m3-server.md`. Bindings only (AGENTS.md "Structure"): every function converts types and
//! calls proto/core; no protocol rules, LWW, geometry or pixel logic lives here. The browser rasterizes the
//! returned geometry with Canvas2D (`web/src/viewer.ts`).
//!
//! JS conventions: ids are 26-char ULID strings; seqs and nonces are `bigint` (`u64`); flat geometry arrays
//! are copied out as `Float32Array`/`Uint32Array` (layout in [`draw_list`]).
#![allow(
    clippy::todo,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::unused_self,
    unused_variables,
    dead_code,
    reason = "M3 contract stubs"
)]

pub mod draw_list;
pub mod encode;
pub mod grain;
pub mod received;
pub mod viewer;

pub use draw_list::{DrawKind, DrawList};
pub use received::{MessageKind, NotebookEntry, PageEntry, Received};
pub use viewer::Viewer;
