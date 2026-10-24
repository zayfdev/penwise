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
use super::params_fields::{
    invalid, read_dynamics, read_one_euro, read_outline, read_stamp, write_f32s,
};
use super::varint::{Reader, write_sv32, write_uv32};
use super::{finish_decode, read_header, write_header, write_trailer};
use crate::brush::{BlendMode, BrushFamily, BrushParams, Color};
use crate::sample::{AZIMUTH_GRID_HALF, GridSample, Sample, TILT_GRID_MAX};
use crate::smoothing::SmoothingParams;

/// Stroke blob magic.
pub const STROKE_MAGIC: [u8; 4] = *b"PWSK";
/// Current stroke blob layout version.
pub const STROKE_VERSION: u16 = 1;

/// Largest |x|, |y| grid value (2²⁰ px · 16).
const POSITION_GRID_LIMIT: i64 = 1 << 24;
/// Largest pressure grid value.
const PRESSURE_GRID_MAX: i64 = 4096;
/// Smallest encoded size of one sample (six 1-byte varints); bounds pre-allocation on hostile counts.
const MIN_SAMPLE_BYTES: usize = 6;

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
    let start = out.len();
    write_header(out, STROKE_MAGIC, STROKE_VERSION);
    encode_params(&record.params, out);
    out.extend_from_slice(&record.seed.to_le_bytes());
    let grid = record.samples.iter().copied().map(Sample::to_grid);
    write_samples(grid, record.samples.len(), out);
    write_trailer(out, start);
}

/// Decodes a complete v1 blob (exactly one blob, nothing after the trailer).
///
/// # Errors
/// `BadMagic`, `UnsupportedVersion`, `UnexpectedEof`, `VarintOverflow`, `ChecksumMismatch` (verified before
/// field validation), `TrailingBytes`, `InvalidValue` (unknown enum codes, non-finite floats), `OutOfRange`
/// (grid values outside the [`GridSample`] ranges after delta accumulation, or delta accumulation overflow).
pub fn decode_stroke(bytes: &[u8]) -> Result<StrokeRecord, CodecError> {
    let mut r = Reader::new(bytes);
    read_header(&mut r, STROKE_MAGIC, STROKE_VERSION)?;
    let body = decode_body(&mut r).map(|record| (record, r.position()));
    finish_decode(bytes, body)
}

/// Everything between the header and the trailer.
fn decode_body(r: &mut Reader<'_>) -> Result<StrokeRecord, CodecError> {
    let params = decode_params(r)?;
    let seed = r.u64_le()?;
    let mut samples = Vec::new();
    decode_samples(r, &mut samples)?;
    Ok(StrokeRecord {
        params,
        seed,
        samples,
    })
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
    let color = params.color;
    out.extend_from_slice(&[params.family.code(), color.r, color.g, color.b, color.a]);
    write_f32s(out, &[params.opacity]);
    out.push(params.blend.code());
    write_f32s(out, &[params.size_min, params.size_max, params.size_gamma]);
    let dynamics = params.dynamics;
    let (curve, velocity, tilt) = (dynamics.pressure, dynamics.velocity, dynamics.tilt);
    write_f32s(out, &[curve.floor, curve.saturation, curve.gamma]);
    write_f32s(
        out,
        &[velocity.v0, velocity.v1, velocity.size, velocity.opacity],
    );
    write_f32s(
        out,
        &[
            tilt.theta0,
            tilt.theta1,
            tilt.max_aspect,
            tilt.max_radius_scale,
        ],
    );
    write_f32s(out, &[tilt.side_hardness, tilt.side_alpha]);
    for euro in [params.smoothing.position, params.smoothing.pressure] {
        write_f32s(out, &[euro.min_cutoff_hz, euro.beta, euro.d_cutoff_hz]);
    }
    let outline = params.outline;
    let nib = outline.nib;
    write_f32s(
        out,
        &[outline.smoothing, outline.taper_start, outline.taper_end],
    );
    out.extend_from_slice(&[outline.cap_start.code(), outline.cap_end.code()]);
    write_f32s(out, &[nib.angle, nib.aspect, nib.thickness, nib.fixation]);
    let stamp = params.stamp;
    let jitter = stamp.jitter;
    write_f32s(
        out,
        &[
            stamp.flow,
            stamp.flow_pressure,
            stamp.hardness,
            stamp.spacing,
        ],
    );
    write_f32s(
        out,
        &[stamp.scatter, jitter.size, jitter.opacity, jitter.angle],
    );
    write_f32s(out, &[stamp.dabs_per_second]);
    match stamp.grain {
        Some(grain) => {
            out.push(1);
            write_f32s(out, &[grain.scale, grain.strength, grain.softness]);
        }
        None => out.push(0),
    }
    match stamp.spray {
        Some(spray) => {
            out.push(1);
            write_uv32(out, spray.particles_max);
            write_f32s(out, &[spray.particle_size_min, spray.particle_size_max]);
            out.push(spray.distribution.code());
        }
        None => out.push(0),
    }
}

/// Reads params written by [`encode_params`].
///
/// # Errors
/// `UnexpectedEof`, `VarintOverflow`, `InvalidValue` (unknown codes, flags other than 0/1, non-finite floats).
pub fn decode_params(reader: &mut Reader<'_>) -> Result<BrushParams, CodecError> {
    let r = reader;
    let family = BrushFamily::from_code(r.u8()?).ok_or(invalid("family"))?;
    let color = Color {
        r: r.u8()?,
        g: r.u8()?,
        b: r.u8()?,
        a: r.u8()?,
    };
    let opacity = r.f32_le("opacity")?;
    let blend = BlendMode::from_code(r.u8()?).ok_or(invalid("blend"))?;
    Ok(BrushParams {
        family,
        color,
        opacity,
        blend,
        size_min: r.f32_le("size_min")?,
        size_max: r.f32_le("size_max")?,
        size_gamma: r.f32_le("size_gamma")?,
        dynamics: read_dynamics(r)?,
        smoothing: SmoothingParams {
            position: read_one_euro(r)?,
            pressure: read_one_euro(r)?,
        },
        outline: read_outline(r)?,
        stamp: read_stamp(r)?,
    })
}

/// Appends grid samples as the `count` + sample section of the layout above.
pub fn encode_samples(samples: &[GridSample], out: &mut Vec<u8>) {
    write_samples(samples.iter().copied(), samples.len(), out);
}

/// Writes `count` then the first `min(len, u32::MAX)` samples (absolute, then wrapping deltas).
fn write_samples(samples: impl Iterator<Item = GridSample>, len: usize, out: &mut Vec<u8>) {
    let count = u32::try_from(len).unwrap_or(u32::MAX);
    write_uv32(out, count);
    let mut prev: Option<GridSample> = None;
    for (s, _) in samples.zip(0..count) {
        match prev {
            None => {
                write_sv32(out, s.x);
                write_sv32(out, s.y);
                write_uv32(out, u32::from(s.pressure));
                write_uv32(out, u32::from(s.tilt));
                write_sv32(out, i32::from(s.azimuth));
                write_uv32(out, s.t_us);
            }
            Some(p) => {
                write_sv32(out, s.x.wrapping_sub(p.x));
                write_sv32(out, s.y.wrapping_sub(p.y));
                write_sv32(out, i32::from(s.pressure) - i32::from(p.pressure));
                write_sv32(out, i32::from(s.tilt) - i32::from(p.tilt));
                write_sv32(out, i32::from(s.azimuth) - i32::from(p.azimuth));
                write_sv32(out, s.t_us.wrapping_sub(p.t_us).cast_signed());
            }
        }
        prev = Some(s);
    }
}

/// Reads the `count` + sample section, appending decoded samples to `out`.
///
/// # Errors
/// `UnexpectedEof`, `VarintOverflow`, `OutOfRange` (see [`decode_stroke`]).
pub fn decode_samples(reader: &mut Reader<'_>, out: &mut Vec<Sample>) -> Result<(), CodecError> {
    let r = reader;
    let count = r.uv32()?;
    let hint = usize::try_from(count).unwrap_or(usize::MAX);
    out.reserve(hint.min(r.remaining() / MIN_SAMPLE_BYTES));
    let mut prev: Option<GridSample> = None;
    for _ in 0..count {
        let s = match prev {
            None => GridSample {
                x: position(i64::from(r.sv32()?), "x")?,
                y: position(i64::from(r.sv32()?), "y")?,
                pressure: ranged(i64::from(r.uv32()?), 0, PRESSURE_GRID_MAX, "pressure")?,
                tilt: ranged(i64::from(r.uv32()?), 0, i64::from(TILT_GRID_MAX), "tilt")?,
                azimuth: azimuth(i64::from(r.sv32()?))?,
                t_us: r.uv32()?,
            },
            Some(p) => GridSample {
                x: position(i64::from(p.x) + i64::from(r.sv32()?), "x")?,
                y: position(i64::from(p.y) + i64::from(r.sv32()?), "y")?,
                pressure: ranged(
                    i64::from(p.pressure) + i64::from(r.sv32()?),
                    0,
                    PRESSURE_GRID_MAX,
                    "pressure",
                )?,
                tilt: ranged(
                    i64::from(p.tilt) + i64::from(r.sv32()?),
                    0,
                    i64::from(TILT_GRID_MAX),
                    "tilt",
                )?,
                azimuth: azimuth(i64::from(p.azimuth) + i64::from(r.sv32()?))?,
                t_us: p.t_us.wrapping_add(r.sv32()?.cast_unsigned()),
            },
        };
        out.push(s.to_sample());
        prev = Some(s);
    }
    Ok(())
}

/// `v` as `T` if `lo ≤ v ≤ hi`, else `OutOfRange { field }`.
fn ranged<T: TryFrom<i64>>(v: i64, lo: i64, hi: i64, field: &'static str) -> Result<T, CodecError> {
    if (lo..=hi).contains(&v) {
        T::try_from(v).map_err(|_| CodecError::OutOfRange { field })
    } else {
        Err(CodecError::OutOfRange { field })
    }
}

fn position(v: i64, field: &'static str) -> Result<i32, CodecError> {
    ranged(v, -POSITION_GRID_LIMIT, POSITION_GRID_LIMIT, field)
}

fn azimuth(v: i64) -> Result<i16, CodecError> {
    let half = i64::from(AZIMUTH_GRID_HALF);
    ranged(v, -(half - 1), half, "azimuth")
}

#[cfg(test)]
mod tests;
