//! Versioned edit blob (the payload `penwise-proto` wraps in its op envelope).
//!
//! Owner: slice e (M2).
//!
//! Layout v5 (conventions as in [`super::item`]; v1–v4 blobs still decode, they just cannot carry the newer
//! fields (frame fields in v2, mark fields and v2 item bodies in v3, frame patch/label/group/chosen and v3 item
//! bodies in v4, frame extent/document height and v4 item bodies in v5)):
//! ```text
//! magic     4   b"PWED"
//! version   u16 = 4
//! body          tag u8: 0 Add | 1 Delete | 2 Update
//!   Add         item body (item::encode_item_body)
//!   Delete      id
//!   Update      id; field mask uv32 (bit Field::code set ⇔ field present); then each present value in field-code
//!               order: Deleted u8 0/1 | Layer u8 | Z u64 | Transform f32×6 | Brush params | ShapeFill flag u8
//!               [+ rgba] | Paint rgba + opacity f32 | Polygons count uv + polygons | ShapeKind (as in item)
//!               | ShapeSize f32×2 | Text len uv + UTF-8 | TextBox f32×2 | FontSize f32
//!               | FramePreset preset u8 + css_w uv + css_h uv | FrameRoute len uv + UTF-8
//!               | MarkStatus status u8 | MarkStrokes count uv + ids
//!               | FramePatch flag u8 [+ len uv + UTF-8] | FrameLabel len uv + UTF-8
//!               | FrameGroup flag u8 [+ len uv + UTF-8] | FrameChosen u8 0/1
//!               | FrameExtent u8 (0 viewport, 1 full) | FrameDocHeight uv
//! trailer   u64 FNV-1a 64
//! ```
//! `Add` carries an item body at the item layout the version implies (v1/v2 → item v1, v3 → item v2, v4 → item
//! v3, v5 → item v4). Mask bits beyond the version's known fields are `InvalidValue` (a newer writer must bump the version).

use super::error::CodecError;
use super::item::fields::{
    read_affine, read_count, read_flag, read_id, read_layer, read_opt_color, read_paint, read_pair,
    read_polygons, read_shape_kind, read_str, read_z, write_affine, write_flag, write_id,
    write_len, write_opt_color, write_paint, write_polygons, write_shape_kind, write_str, write_z,
};
use super::item::{
    ITEM_VERSION, ITEM_VERSION_V1, ITEM_VERSION_V2, ITEM_VERSION_V3, decode_item_body_at,
    encode_item_body_at,
};
use super::params_fields::{invalid, write_f32s};
use super::stroke::{decode_params, encode_params};
use super::varint::{Reader, write_uv32};
use super::{finish_decode, read_header_any, write_header, write_trailer};
use crate::edit::{Edit, ItemPatch};
use crate::item::{FrameExtent, FramePreset, FrameSize, ItemId};
use crate::marks::MarkStatus;

/// Edit blob magic.
pub const EDIT_MAGIC: [u8; 4] = *b"PWED";
/// Version this build needs for the frame extent and document height fields (mask bits 21 and 22) and `Add`
/// blobs carrying a full-length frame (item layout v4). Edits that need less are written at the smallest version
/// that carries them, so old blobs re-encode byte for byte and older readers keep working.
pub const EDIT_VERSION: u16 = 5;
/// Frame patch/label/group/chosen fields (mask bits 17–20) and `Add` blobs carrying frame variants (item
/// layout v3).
pub const EDIT_VERSION_V4: u16 = 4;
/// Mark fields (mask bits 15 and 16), item layout v2.
pub const EDIT_VERSION_V3: u16 = 3;
/// Frame fields (`FramePreset`/`FrameRoute`, mask bits 13 and 14), item layout v1.
pub const EDIT_VERSION_V2: u16 = 2;
/// The baseline version: no frame fields, mask bits `< 13`.
pub const EDIT_VERSION_V1: u16 = 1;

const TAG_ADD: u8 = 0;
const TAG_DELETE: u8 = 1;
const TAG_UPDATE: u8 = 2;
/// Number of known fields: mask bits `0..FIELD_COUNT` (bit = `Field::code`).
const FIELD_COUNT: u32 = 23;
/// Known fields of [`EDIT_VERSION_V4`].
const FIELD_COUNT_V4: u32 = 21;
/// Known fields of [`EDIT_VERSION_V3`].
const FIELD_COUNT_V3: u32 = 17;
/// Known fields of [`EDIT_VERSION_V2`].
const FIELD_COUNT_V2: u32 = 15;
/// Known fields of [`EDIT_VERSION_V1`].
const FIELD_COUNT_V1: u32 = 13;

/// Appends the blob for `edit` at the smallest version that can carry it (see [`version_for`]).
pub fn encode_edit(edit: &Edit, out: &mut Vec<u8>) {
    let start = out.len();
    let version = version_for(edit);
    write_header(out, EDIT_MAGIC, version);
    encode_edit_body_at(edit, version, out);
    write_trailer(out, start);
}

/// The smallest version that carries `edit`: an `Add` of an item that needs item layout v4 (a full-length
/// frame) is v5, of one that needs v3 (a frame variant) is v4, of one that needs v2 (a bound stroke or a mark)
/// is v3, and every other `Add`/`Delete` is v1; an `Update` needs v5 / v4 / v3 / v2 / v1 by its highest set
/// field code.
fn version_for(edit: &Edit) -> u16 {
    let Edit::Update { patch, .. } = edit else {
        return match edit {
            Edit::Add(item) if needs_item_v4(item) => EDIT_VERSION,
            Edit::Add(item) if needs_item_v3(item) => EDIT_VERSION_V4,
            Edit::Add(item) if needs_item_v2(item) => EDIT_VERSION_V3,
            _ => EDIT_VERSION_V1,
        };
    };
    let highest = patch
        .fields()
        .iter()
        .map(|field| u32::from(field.code()))
        .max()
        .unwrap_or(0);
    if highest >= FIELD_COUNT_V4 {
        EDIT_VERSION
    } else if highest >= FIELD_COUNT_V3 {
        EDIT_VERSION_V4
    } else if highest >= FIELD_COUNT_V2 {
        EDIT_VERSION_V3
    } else if highest >= FIELD_COUNT_V1 {
        EDIT_VERSION_V2
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
    let version = read_header_any(
        &mut r,
        EDIT_MAGIC,
        &[
            EDIT_VERSION_V1,
            EDIT_VERSION_V2,
            EDIT_VERSION_V3,
            EDIT_VERSION_V4,
            EDIT_VERSION,
        ],
    )?;
    let fields = match version {
        EDIT_VERSION_V1 => FIELD_COUNT_V1,
        EDIT_VERSION_V2 => FIELD_COUNT_V2,
        EDIT_VERSION_V3 => FIELD_COUNT_V3,
        EDIT_VERSION_V4 => FIELD_COUNT_V4,
        _ => FIELD_COUNT,
    };
    let body = decode_body(&mut r, fields, item_version(version)).map(|edit| (edit, r.position()));
    finish_decode(bytes, body)
}

/// Appends the edit body only (no header/trailer), at the current layout version.
pub fn encode_edit_body(edit: &Edit, out: &mut Vec<u8>) {
    encode_edit_body_at(edit, EDIT_VERSION, out);
}

/// [`encode_edit_body`] for a blob written at `version` (its `Add` bodies use the item layout it implies).
fn encode_edit_body_at(edit: &Edit, version: u16, out: &mut Vec<u8>) {
    match edit {
        Edit::Add(item) => {
            out.push(TAG_ADD);
            encode_item_body_at(item, item_version(version), out);
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
    decode_body(reader, FIELD_COUNT, ITEM_VERSION)
}

/// True when `item` cannot be written at item layout v1: a bound stroke or a mark.
fn needs_item_v2(item: &crate::item::Item) -> bool {
    match &item.kind {
        crate::item::ItemKind::Mark(_) => true,
        crate::item::ItemKind::Stroke(stroke) => stroke.binding.is_some(),
        _ => false,
    }
}

/// True when `item` cannot be written at item layout v2: a frame with a patch, label, group or chosen flag.
/// (Marks and bound strokes need only v2.)
fn needs_item_v3(item: &crate::item::Item) -> bool {
    match &item.kind {
        crate::item::ItemKind::Frame(frame) => {
            frame.patch.is_some()
                || !frame.label.is_empty()
                || frame.group.is_some()
                || frame.chosen
        }
        _ => false,
    }
}

/// True when `item` cannot be written at item layout v3: a frame that is full length or carries a measured
/// document height.
fn needs_item_v4(item: &crate::item::Item) -> bool {
    match &item.kind {
        crate::item::ItemKind::Frame(frame) => {
            frame.extent == FrameExtent::Full || frame.doc_h != 0
        }
        _ => false,
    }
}

/// The item layout that version implies for its `Add` bodies.
fn item_version(version: u16) -> u16 {
    match version {
        EDIT_VERSION_V1 | EDIT_VERSION_V2 => ITEM_VERSION_V1,
        EDIT_VERSION_V3 => ITEM_VERSION_V2,
        EDIT_VERSION_V4 => ITEM_VERSION_V3,
        _ => ITEM_VERSION,
    }
}

/// [`decode_edit_body`] for a blob whose version allows `fields` mask bits and `item_version` item bodies.
fn decode_body(r: &mut Reader<'_>, fields: u32, item_version: u16) -> Result<Edit, CodecError> {
    match r.u8()? {
        TAG_ADD => Ok(Edit::Add(decode_item_body_at(r, item_version)?)),
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
        p.mark_status.is_some(),
        p.mark_strokes.is_some(),
        p.frame_patch.is_some(),
        p.frame_label.is_some(),
        p.frame_group.is_some(),
        p.frame_chosen.is_some(),
        p.frame_extent.is_some(),
        p.frame_doc_h.is_some(),
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
    if let Some(v) = p.mark_status {
        out.push(v.code());
    }
    if let Some(v) = &p.mark_strokes {
        let n = write_len(out, v.len());
        for id in v.iter().take(n) {
            write_id(out, *id);
        }
    }
    if let Some(v) = &p.frame_patch {
        write_opt_str(out, v.as_deref());
    }
    if let Some(v) = &p.frame_label {
        write_str(out, v);
    }
    if let Some(v) = &p.frame_group {
        write_opt_str(out, v.as_deref());
    }
    if let Some(v) = p.frame_chosen {
        write_flag(out, v);
    }
    if let Some(v) = p.frame_extent {
        out.push(v.code());
    }
    if let Some(v) = p.frame_doc_h {
        write_uv32(out, v);
    }
}

/// `flag u8` + the text when present.
fn write_opt_str(out: &mut Vec<u8>, text: Option<&str>) {
    match text {
        Some(text) => {
            write_flag(out, true);
            write_str(out, text);
        }
        None => write_flag(out, false),
    }
}

fn read_opt_str(r: &mut Reader<'_>, field: &'static str) -> Result<Option<String>, CodecError> {
    if read_flag(r, field)? {
        Ok(Some(read_str(r)?))
    } else {
        Ok(None)
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
        mark_status: has(15).then(|| read_mark_status(r)).transpose()?,
        mark_strokes: has(16).then(|| read_ids(r)).transpose()?,
        frame_patch: has(17)
            .then(|| read_opt_str(r, "frame.patch flag"))
            .transpose()?,
        frame_label: has(18).then(|| read_str(r)).transpose()?,
        frame_group: has(19)
            .then(|| read_opt_str(r, "frame.group flag"))
            .transpose()?,
        frame_chosen: has(20).then(|| read_flag(r, "frame.chosen")).transpose()?,
        frame_extent: has(21).then(|| read_frame_extent(r)).transpose()?,
        frame_doc_h: has(22).then(|| r.uv32()).transpose()?,
    })
}

fn read_frame_extent(r: &mut Reader<'_>) -> Result<FrameExtent, CodecError> {
    FrameExtent::from_code(r.u8()?).ok_or(invalid("frame.extent"))
}

fn read_mark_status(r: &mut Reader<'_>) -> Result<MarkStatus, CodecError> {
    MarkStatus::from_code(r.u8()?).ok_or(invalid("mark.status"))
}

fn read_ids(r: &mut Reader<'_>) -> Result<Vec<ItemId>, CodecError> {
    let (count, cap) = read_count(r, 16)?;
    let mut out = Vec::with_capacity(cap);
    for _ in 0..count {
        out.push(read_id(r)?);
    }
    Ok(out)
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
