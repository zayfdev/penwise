//! Penwise ink core: raw pen samples in, deterministic stroke geometry and pixels out.
//!
//! Pure by design: no I/O, async, clocks, or logging (see `AGENTS.md`).
//!
//! Owner: contract. Only the contract owner edits this file; the M1 contract lives in
//! `docs/contracts/m1-core.md`.
//!
//! Data flow: [`Sample`] → [`Sample::quantize`] → [`smoothing`] → [`dynamics`] → [`outline`] | [`stamp`]
//! → [`StrokeGeometry`] → `raster` (feature `raster`) / [`codec`].
// M1 contract stage: slices replace the `todo!()` bodies. The orchestrator removes this allow-list once every
// slice has landed; implementers must not rely on it for finished code.
#![allow(
    clippy::todo,
    unused_variables,
    dead_code,
    clippy::missing_panics_doc,
    clippy::unused_self,
    clippy::needless_pass_by_value,
    reason = "M1 contract stubs: removed when the last slice lands"
)]

pub mod bounds;
pub mod brush;
pub mod builder;
pub mod codec;
pub mod dynamics;
pub mod float;
pub mod geometry;
pub mod hash;
pub mod outline;
#[cfg(feature = "raster")]
pub mod raster;
pub mod sample;
pub mod smoothing;
pub mod stamp;

pub use bounds::Bounds;
pub use brush::{BlendMode, BrushFamily, BrushParams, Color, OutlineKind, Preset, StampKind};
pub use builder::StrokeBuilder;
pub use codec::CodecError;
pub use geometry::{Dab, GeometryView, Outline, StrokeGeometry, StrokeStyle};
pub use sample::{DigitizerRanges, RawSample, Sample};
