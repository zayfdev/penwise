//! Versioned item blob (storage and sync payload of `AddItem`; PLAN "Data model").
//!
//! Owner: slice e (M2).
//!
//! Layout v1 (fixed-width fields LE; `f32` = `to_bits()` LE through `float::canon`, must be finite on decode;
//! `uv`/`sv` per `varint`; `id` = 16 raw ULID bytes; `pt` = point on the 1/16 px grid as `sv x, sv y`,
//! first point of a run absolute, later points deltas):
//! ```text
//! magic     4   b"PWIT"
//! version   u16 = 1
//! body          see below (also embedded by the edit codec)
//! trailer   u64 FNV-1a 64 of every preceding byte
//!
//! body:   id; kind u8 (ItemKindTag::code); layer u8 (Layer::code); z u64 (i64 two's complement);
//!         transform a b c d e f f32×6; kind data:
//! Stroke  params (stroke::encode_params); seed u64; samples (stroke::encode_samples);
//!         trim flag u8 (0/1) [+ origin id; sample_offset uv; own uv; count uv;
//!                             count × (start uv, end uv, flags u8: bit0 start_cut, bit1 end_cut)]
//! Erase   target id; radius f32; path count uv + pts
//! Shape   kind u8 (ShapeKind::code) [+ RoundedRect radius f32 | Arrow head u8 | Polygon count uv + f32 pairs];
//!         size f32×2; brush (encode_params); fill flag u8 [+ r g b a u8]
//! Fill    paint (r g b a u8, opacity f32); polygon count uv; per polygon: outer ring, hole count uv, holes;
//!         ring = count uv + pts
//! Text    content (len uv + UTF-8); size f32×2; font_size f32; paint; sources count uv + ids
//! ```
//! Grid-stored values (erase paths, fill rings) round-trip exactly only when already on the 1/16 px grid
//! (the producers snap them); everything else round-trips bit-exactly. Decoding validates with
//! `Item::validate`, mapping its failure to `InvalidValue`.

pub(super) mod fields;
mod kind;

use super::error::CodecError;
use super::params_fields::invalid;
use super::varint::{Reader, write_sv32};
use super::{finish_decode, read_header, write_header, write_trailer};
use crate::edit::EditError;
use crate::item::{Item, ItemKindTag};
use crate::sample::{POSITION_LIMIT_PX, POSITION_STEPS_PER_PX};
use fields::{
    read_affine, read_count, read_id, read_layer, read_z, write_affine, write_id, write_len,
    write_z,
};

/// Item blob magic.
pub const ITEM_MAGIC: [u8; 4] = *b"PWIT";
/// Current item blob layout version.
pub const ITEM_VERSION: u16 = 1;

/// Largest |x|, |y| point grid value (2²⁰ px · 16).
const POINT_GRID_LIMIT: i64 = 1 << 24;

/// Appends the v1 blob for `item` (header + body + trailer).
pub fn encode_item(item: &Item, out: &mut Vec<u8>) {
    let start = out.len();
    write_header(out, ITEM_MAGIC, ITEM_VERSION);
    encode_item_body(item, out);
    write_trailer(out, start);
}

/// Decodes exactly one v1 item blob.
///
/// # Errors
/// `BadMagic`, `UnsupportedVersion`, `UnexpectedEof`, `VarintOverflow`, `ChecksumMismatch` (checked before
/// field validation, as in the stroke codec), `TrailingBytes`, `InvalidValue` (unknown tags, flags other than
/// 0/1, non-finite floats, bad UTF-8, `Item::validate` failure), `OutOfRange` (grid or sample values).
pub fn decode_item(bytes: &[u8]) -> Result<Item, CodecError> {
    let mut r = Reader::new(bytes);
    read_header(&mut r, ITEM_MAGIC, ITEM_VERSION)?;
    let body = decode_item_body(&mut r).map(|item| (item, r.position()));
    finish_decode(bytes, body)
}

/// Appends the item body only (no header/trailer), for embedding in edits and `penwise-proto` messages.
pub fn encode_item_body(item: &Item, out: &mut Vec<u8>) {
    write_id(out, item.id);
    out.extend_from_slice(&[item.kind.tag().code(), item.layer.code()]);
    write_z(out, item.z);
    write_affine(out, &item.transform);
    kind::encode_kind(&item.kind, out);
}

/// Reads an item body written by [`encode_item_body`] (validated like [`decode_item`]).
///
/// # Errors
/// As [`decode_item`] minus header/trailer errors.
pub fn decode_item_body(reader: &mut Reader<'_>) -> Result<Item, CodecError> {
    let r = reader;
    let id = read_id(r)?;
    let tag = ItemKindTag::from_code(r.u8()?).ok_or(invalid("kind"))?;
    let layer = read_layer(r)?;
    let z = read_z(r)?;
    let transform = read_affine(r)?;
    let kind = kind::decode_kind(r, tag)?;
    let item = Item {
        id,
        layer,
        z,
        transform,
        kind,
    };
    item.validate().map_err(|e| match e {
        EditError::InvalidValue { field } => invalid(field),
        _ => invalid("item"),
    })?;
    Ok(item)
}

/// Appends a run of grid points: `count uv`, first point absolute (`sv`, 1/16 px), then deltas
/// (`Sample::quantize`-style rounding, clamped to the ±2²⁰ px position limit).
pub fn encode_points(points: &[[f32; 2]], out: &mut Vec<u8>) {
    let n = write_len(out, points.len());
    let mut prev = [0_i32; 2];
    for p in points.iter().take(n) {
        let g = [to_grid(p[0]), to_grid(p[1])];
        // |g| ≤ 2²⁴, so deltas never wrap; the first point is a delta from the origin.
        write_sv32(out, g[0].wrapping_sub(prev[0]));
        write_sv32(out, g[1].wrapping_sub(prev[1]));
        prev = g;
    }
}

/// Reads a run written by [`encode_points`], appending to `out`.
///
/// # Errors
/// `UnexpectedEof`, `VarintOverflow`, `OutOfRange` (beyond the position limit after accumulation).
pub fn decode_points(reader: &mut Reader<'_>, out: &mut Vec<[f32; 2]>) -> Result<(), CodecError> {
    let r = reader;
    // Two 1-byte varints per point at least.
    let (count, cap) = read_count(r, 2)?;
    out.reserve(cap);
    let mut prev = [0_i64; 2];
    for _ in 0..count {
        let x = prev[0] + i64::from(r.sv32()?);
        let y = prev[1] + i64::from(r.sv32()?);
        out.push([from_grid(x, "point.x")?, from_grid(y, "point.y")?]);
        prev = [x, y];
    }
    Ok(())
}

/// Page px → 1/16 px grid: NaN → 0, clamped to ±2²⁰ px, rounded half away from zero (as `Sample::to_grid`).
#[allow(
    clippy::cast_possible_truncation,
    reason = "clamped to ±2^24 grid steps before the cast"
)]
fn to_grid(v: f32) -> i32 {
    let v = if v.is_nan() {
        0.0
    } else {
        v.clamp(-POSITION_LIMIT_PX, POSITION_LIMIT_PX)
    };
    (v * POSITION_STEPS_PER_PX).round() as i32
}

/// Grid value → px; `OutOfRange { field }` beyond ±2²⁴.
#[allow(clippy::cast_precision_loss, reason = "|grid| ≤ 2^24, exact in f32")]
fn from_grid(v: i64, field: &'static str) -> Result<f32, CodecError> {
    if (-POINT_GRID_LIMIT..=POINT_GRID_LIMIT).contains(&v) {
        Ok(v as f32 / POSITION_STEPS_PER_PX)
    } else {
        Err(CodecError::OutOfRange { field })
    }
}
