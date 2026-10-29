//! Versioned edit blob (the payload `penwise-proto` wraps in its op envelope).
//!
//! Owner: slice e (M2).
//!
//! Layout v2 (conventions as in [`super::item`]; v1 blobs still decode, they just cannot carry the frame
//! fields):
//! ```text
//! magic     4   b"PWED"
//! version   u16 = 2
//! body          tag u8: 0 Add | 1 Delete | 2 Update
//!   Add         item body (item::encode_item_body)
//!   Delete      id
//!   Update      id; field mask uv32 (bit Field::code set ⇔ field present); then each present value in field-code
//!               order: Deleted u8 0/1 | Layer u8 | Z u64 | Transform f32×6 | Brush params | ShapeFill flag u8
//!               [+ rgba] | Paint rgba + opacity f32 | Polygons count uv + polygons | ShapeKind (as in item)
//!               | ShapeSize f32×2 | Text len uv + UTF-8 | TextBox f32×2 | FontSize f32
//!               | FramePreset preset u8 + css_w uv + css_h uv | FrameRoute len uv + UTF-8
//! trailer   u64 FNV-1a 64
//! ```
//! Mask bits beyond the version's known fields are `InvalidValue` (a newer writer must bump the version).

use super::error::CodecError;
use super::item::fields::{
    read_affine, read_flag, read_id, read_layer, read_opt_color, read_paint, read_pair,
    read_polygons, read_shape_kind, read_str, read_z, write_affine, write_flag, write_id,
    write_opt_color, write_paint, write_polygons, write_shape_kind, write_str, write_z,
};
use super::item::{decode_item_body, encode_item_body};
use super::params_fields::{invalid, write_f32s};
use super::stroke::{decode_params, encode_params};
use super::varint::{Reader, write_uv32};
use super::{finish_decode, read_header_any, write_header, write_trailer};
use crate::edit::{Edit, ItemPatch};
use crate::item::{FramePreset, FrameSize};

/// Edit blob magic.
pub const EDIT_MAGIC: [u8; 4] = *b"PWED";
/// Version this build needs for `FramePreset`/`FrameRoute` (mask bits 13 and 14). Edits that set neither are
/// written at [`EDIT_VERSION_V1`], so blobs from before frames existed re-encode byte for byte and older
/// readers keep working.
pub const EDIT_VERSION: u16 = 2;
/// The baseline version: no frame fields, mask bits `< 13`.
pub const EDIT_VERSION_V1: u16 = 1;

const TAG_ADD: u8 = 0;
const TAG_DELETE: u8 = 1;
const TAG_UPDATE: u8 = 2;
/// Number of known fields: mask bits `0..FIELD_COUNT` (bit = `Field::code`).
const FIELD_COUNT: u32 = 15;
/// Known fields of [`EDIT_VERSION_V1`].
const FIELD_COUNT_V1: u32 = 13;

/// Appends the blob for `edit` at the smallest version that can carry it (see [`version_for`]).
pub fn encode_edit(edit: &Edit, out: &mut Vec<u8>) {
    let start = out.len();
    write_header(out, EDIT_MAGIC, version_for(edit));
    encode_edit_body(edit, out);
    write_trailer(out, start);
}

/// `EDIT_VERSION` only when the patch sets a field that version 1 cannot carry; `EDIT_VERSION_V1` otherwise
/// (`Add`/`Delete` and the pre-frame fields are all valid v1).
fn version_for(edit: &Edit) -> u16 {
    let Edit::Update { patch, .. } = edit else {
        return EDIT_VERSION_V1;
    };
    if patch
        .fields()
        .iter()
        .any(|field| field.code() >= u8::try_from(FIELD_COUNT_V1).unwrap_or(u8::MAX))
    {
        EDIT_VERSION
    } else {
        EDIT_VERSION_V1
    }
}

/// Decodes exactly one edit blob of any supported version. Patches are decoded as given (field applicability
/// is the scene's check, not the codec's).
///
/// # Errors
/// As [`super::item::decode_item`]; plus `InvalidValue` for an unknown edit tag or unknown mask bits,
/// `UnsupportedVersion` for a version this build does not know.
pub fn decode_edit(bytes: &[u8]) -> Result<Edit, CodecError> {
    let mut r = Reader::new(bytes);
    let version = read_header_any(&mut r, EDIT_MAGIC, &[EDIT_VERSION_V1, EDIT_VERSION])?;
    let fields = if version == EDIT_VERSION_V1 {
        FIELD_COUNT_V1
    } else {
        FIELD_COUNT
    };
    let body = decode_body(&mut r, fields).map(|edit| (edit, r.position()));
    finish_decode(bytes, body)
}

/// Appends the edit body only (no header/trailer).
pub fn encode_edit_body(edit: &Edit, out: &mut Vec<u8>) {
    match edit {
        Edit::Add(item) => {
            out.push(TAG_ADD);
            encode_item_body(item, out);
        }
        Edit::Delete(id) => {
            out.push(TAG_DELETE);
            write_id(out, *id);
        }
        Edit::Update { id, patch } => {
            out.push(TAG_UPDATE);
            write_id(out, *id);
            encode_patch(patch, out);
        }
    }
}

/// Reads an edit body written by [`encode_edit_body`].
///
/// # Errors
/// As [`decode_edit`] minus header/trailer errors.
pub fn decode_edit_body(reader: &mut Reader<'_>) -> Result<Edit, CodecError> {
    decode_body(reader, FIELD_COUNT)
}

/// [`decode_edit_body`] for a blob whose version allows `fields` mask bits.
fn decode_body(r: &mut Reader<'_>, fields: u32) -> Result<Edit, CodecError> {
    match r.u8()? {
        TAG_ADD => Ok(Edit::Add(decode_item_body(r)?)),
        TAG_DELETE => Ok(Edit::Delete(read_id(r)?)),
        TAG_UPDATE => {
            let id = read_id(r)?;
            let patch = decode_patch(r, fields)?;
            Ok(Edit::Update { id, patch })
        }
        _ => Err(invalid("edit tag")),
    }
}

/// Field mask: bit `i` set ⇔ the field with code `i` is present (codes in `Field` order).
fn field_mask(p: &ItemPatch) -> u32 {
    let present = [
        p.deleted.is_some(),
        p.layer.is_some(),
        p.z.is_some(),
        p.transform.is_some(),
        p.brush.is_some(),
        p.shape_fill.is_some(),
        p.paint.is_some(),
        p.polygons.is_some(),
        p.shape_kind.is_some(),
        p.shape_size.is_some(),
        p.text.is_some(),
        p.text_box.is_some(),
        p.font_size.is_some(),
        p.frame_preset.is_some(),
        p.frame_route.is_some(),
    ];
    (0_u32..).zip(present).fold(
        0,
        |mask, (bit, on)| if on { mask | (1 << bit) } else { mask },
    )
}

/// Mask, then each present value in field-code order.
fn encode_patch(p: &ItemPatch, out: &mut Vec<u8>) {
    write_uv32(out, field_mask(p));
    if let Some(v) = p.deleted {
        write_flag(out, v);
    }
    if let Some(v) = p.layer {
        out.push(v.code());
    }
    if let Some(v) = p.z {
        write_z(out, v);
    }
    if let Some(v) = &p.transform {
        write_affine(out, v);
    }
    if let Some(v) = &p.brush {
        encode_params(v, out);
    }
    if let Some(v) = p.shape_fill {
        write_opt_color(out, v);
    }
    if let Some(v) = p.paint {
        write_paint(out, v);
    }
    if let Some(v) = &p.polygons {
        write_polygons(out, v);
    }
    if let Some(v) = &p.shape_kind {
        write_shape_kind(out, v);
    }
    if let Some(v) = &p.shape_size {
        write_f32s(out, v);
    }
    if let Some(v) = &p.text {
        write_str(out, v);
    }
    if let Some(v) = &p.text_box {
        write_f32s(out, v);
    }
    if let Some(v) = p.font_size {
        write_f32s(out, &[v]);
    }
    if let Some(v) = &p.frame_preset {
        write_frame_size(out, v);
    }
    if let Some(v) = &p.frame_route {
        write_str(out, v);
    }
}

fn decode_patch(r: &mut Reader<'_>, fields: u32) -> Result<ItemPatch, CodecError> {
    let mask = r.uv32()?;
    if mask >> fields != 0 {
        return Err(invalid("field mask"));
    }
    let has = |bit: u32| mask & (1 << bit) != 0;
    // Struct fields are evaluated in source order, which is field-code order.
    Ok(ItemPatch {
        deleted: has(0).then(|| read_flag(r, "deleted")).transpose()?,
        layer: has(1).then(|| read_layer(r)).transpose()?,
        z: has(2).then(|| read_z(r)).transpose()?,
        transform: has(3).then(|| read_affine(r)).transpose()?,
        brush: has(4).then(|| decode_params(r)).transpose()?,
        shape_fill: has(5).then(|| read_opt_color(r)).transpose()?,
        paint: has(6).then(|| read_paint(r)).transpose()?,
        polygons: has(7).then(|| read_polygons(r)).transpose()?,
        shape_kind: has(8).then(|| read_shape_kind(r)).transpose()?,
        shape_size: has(9).then(|| read_pair(r, "shape.size")).transpose()?,
        text: has(10).then(|| read_str(r)).transpose()?,
        text_box: has(11).then(|| read_pair(r, "text.size")).transpose()?,
        font_size: has(12).then(|| r.f32_le("text.font_size")).transpose()?,
        frame_preset: has(13).then(|| read_frame_size(r)).transpose()?,
        frame_route: has(14).then(|| read_str(r)).transpose()?,
    })
}

/// `preset u8` + `css_w uv` + `css_h uv` (one register).
fn write_frame_size(out: &mut Vec<u8>, size: &FrameSize) {
    out.push(size.preset.code());
    write_uv32(out, size.css_w);
    write_uv32(out, size.css_h);
}

fn read_frame_size(r: &mut Reader<'_>) -> Result<FrameSize, CodecError> {
    let preset = FramePreset::from_code(r.u8()?).ok_or(invalid("frame.preset"))?;
    Ok(FrameSize {
        preset,
        css_w: r.uv32()?,
        css_h: r.uv32()?,
    })
}
