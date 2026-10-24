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

#![allow(
    clippy::todo,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    unused_variables,
    dead_code,
    reason = "M2 contract stubs; slice e removes this"
)]

use super::error::CodecError;
use super::varint::Reader;
use crate::item::Item;

/// Item blob magic.
pub const ITEM_MAGIC: [u8; 4] = *b"PWIT";
/// Current item blob layout version.
pub const ITEM_VERSION: u16 = 1;

/// Appends the v1 blob for `item` (header + body + trailer).
pub fn encode_item(item: &Item, out: &mut Vec<u8>) {
    todo!()
}

/// Decodes exactly one v1 item blob.
///
/// # Errors
/// `BadMagic`, `UnsupportedVersion`, `UnexpectedEof`, `VarintOverflow`, `ChecksumMismatch` (checked before
/// field validation, as in the stroke codec), `TrailingBytes`, `InvalidValue` (unknown tags, flags other than
/// 0/1, non-finite floats, bad UTF-8, `Item::validate` failure), `OutOfRange` (grid or sample values).
pub fn decode_item(bytes: &[u8]) -> Result<Item, CodecError> {
    todo!()
}

/// Appends the item body only (no header/trailer), for embedding in edits and `penwise-proto` messages.
pub fn encode_item_body(item: &Item, out: &mut Vec<u8>) {
    todo!()
}

/// Reads an item body written by [`encode_item_body`] (validated like [`decode_item`]).
///
/// # Errors
/// As [`decode_item`] minus header/trailer errors.
pub fn decode_item_body(reader: &mut Reader<'_>) -> Result<Item, CodecError> {
    todo!()
}

/// Appends a run of grid points: `count uv`, first point absolute (`sv`, 1/16 px), then deltas
/// (`Sample::quantize`-style rounding, clamped to the ±2²⁰ px position limit).
pub fn encode_points(points: &[[f32; 2]], out: &mut Vec<u8>) {
    todo!()
}

/// Reads a run written by [`encode_points`], appending to `out`.
///
/// # Errors
/// `UnexpectedEof`, `VarintOverflow`, `OutOfRange` (beyond the position limit after accumulation).
pub fn decode_points(reader: &mut Reader<'_>, out: &mut Vec<[f32; 2]>) -> Result<(), CodecError> {
    todo!()
}
