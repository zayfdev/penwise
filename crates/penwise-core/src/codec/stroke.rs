//! Versioned stroke blob: brush params + seed + quantized samples (PLAN "Data model": the stroke's source of
//! truth; geometry is always re-derived).
//!
//! Owner: slice d.
//!
//! Layout v1 (all fixed-width fields little-endian; `f32` = `to_bits()` LE, must be finite; `uv`/`sv` per
//! `varint`):
//! ```text
//! magic     4   b"PWSK"
//! version   u16 = 1
//! params        see `encode_params`
//! seed      u64
//! count     uv  number of samples n (may be 0)
//! sample 0      sv x, sv y, uv pressure, uv tilt, sv azimuth, uv t_us        (GridSample, absolute)
//! sample i      sv Δx, sv Δy, sv Δpressure, sv Δtilt, sv Δazimuth, sv Δt_us  (vs sample i−1; Δt as wrapping i32)
//! trailer   u64 FNV-1a 64 of every preceding byte
//! ```
//! Samples are stored on the [`GridSample`] grid, so `decode(encode(r))` returns samples equal to
//! `Sample::quantize` of the input and the builder sees identical inputs live and after storage.

use super::error::CodecError;
use super::varint::Reader;
use crate::brush::BrushParams;
use crate::sample::{GridSample, Sample};

/// Stroke blob magic.
pub const STROKE_MAGIC: [u8; 4] = *b"PWSK";
/// Current stroke blob layout version.
pub const STROKE_VERSION: u16 = 1;

/// Decoded stroke blob.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeRecord {
    pub params: BrushParams,
    pub seed: u64,
    /// Always on the quantization grid after decoding; encoding quantizes (`Sample::to_grid`).
    pub samples: Vec<Sample>,
}

/// Appends the v1 blob for `record` to `out` (samples quantized via `Sample::to_grid`).
pub fn encode_stroke(record: &StrokeRecord, out: &mut Vec<u8>) {
    todo!()
}

/// Decodes a complete v1 blob (exactly one blob, nothing after the trailer).
///
/// # Errors
/// `BadMagic`, `UnsupportedVersion`, `UnexpectedEof`, `VarintOverflow`, `ChecksumMismatch` (verified before
/// field validation), `TrailingBytes`, `InvalidValue` (unknown enum codes, non-finite floats), `OutOfRange`
/// (grid values outside the [`GridSample`] ranges after delta accumulation, or delta accumulation overflow).
pub fn decode_stroke(bytes: &[u8]) -> Result<StrokeRecord, CodecError> {
    todo!()
}

/// Appends `params` in fixed field order (shared with saved custom brushes):
/// `family.code() u8; color r g b a u8; opacity f32; blend.code() u8; size_min, size_max, size_gamma f32;`
/// `pressure floor, saturation, gamma f32; velocity v0, v1, size, opacity f32;`
/// `tilt theta0, theta1, max_aspect, max_radius_scale, side_hardness, side_alpha f32;`
/// `smoothing position (min_cutoff_hz, beta, d_cutoff_hz) f32×3, pressure (same) f32×3;`
/// `outline smoothing, taper_start, taper_end f32; cap_start.code(), cap_end.code() u8;`
/// `nib angle, aspect, thickness, fixation f32;`
/// `stamp flow, flow_pressure, hardness, spacing, scatter f32; jitter size, opacity, angle f32;`
/// `dabs_per_second f32; grain flag u8 (0/1) [+ scale, strength, softness f32];`
/// `spray flag u8 (0/1) [+ particles_max uv, particle_size_min, particle_size_max f32, distribution.code() u8]`.
/// Floats are written through `float::canon`; a non-finite param is written as `+0.0`.
pub fn encode_params(params: &BrushParams, out: &mut Vec<u8>) {
    todo!()
}

/// Reads params written by [`encode_params`].
///
/// # Errors
/// `UnexpectedEof`, `VarintOverflow`, `InvalidValue` (unknown codes, flags other than 0/1, non-finite floats).
pub fn decode_params(reader: &mut Reader<'_>) -> Result<BrushParams, CodecError> {
    todo!()
}

/// Appends grid samples as the `count` + sample section of the layout above.
pub fn encode_samples(samples: &[GridSample], out: &mut Vec<u8>) {
    todo!()
}

/// Reads the `count` + sample section, appending decoded samples to `out`.
///
/// # Errors
/// `UnexpectedEof`, `VarintOverflow`, `OutOfRange` (see [`decode_stroke`]).
pub fn decode_samples(reader: &mut Reader<'_>, out: &mut Vec<Sample>) -> Result<(), CodecError> {
    todo!()
}
