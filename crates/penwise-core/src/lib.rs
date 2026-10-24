//! Penwise ink core: raw pen samples in, deterministic stroke geometry and pixels out.
//!
//! Pure by design: no I/O, async, clocks, or logging (see `AGENTS.md`).
//!
//! Owner: contract. Only the contract owner edits this file; the M1 contract lives in
//! `docs/contracts/m1-core.md`.
//!
//! Data flow: [`Sample`] → [`Sample::quantize`] → [`smoothing`] → [`dynamics`] → [`outline`] | [`stamp`]
//! → [`StrokeGeometry`] → `raster` (feature `raster`) / [`codec`].
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
