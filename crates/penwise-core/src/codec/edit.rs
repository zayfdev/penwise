//! Versioned edit blob (the payload `penwise-proto` wraps in its op envelope).
//!
//! Owner: slice e (M2).
//!
//! Layout v1 (conventions as in [`super::item`]):
//! ```text
//! magic     4   b"PWED"
//! version   u16 = 1
//! body          tag u8: 0 Add | 1 Delete | 2 Update
//!   Add         item body (item::encode_item_body)
//!   Delete      id
//!   Update      id; field mask uv32 (bit Field::code set ⇔ field present); then each present value in field-code
//!               order: Deleted u8 0/1 | Layer u8 | Z u64 | Transform f32×6 | Brush params | ShapeFill flag u8
//!               [+ rgba] | Paint rgba + opacity f32 | Polygons count uv + polygons | ShapeKind (as in item)
//!               | ShapeSize f32×2 | Text len uv + UTF-8 | TextBox f32×2 | FontSize f32
//! trailer   u64 FNV-1a 64
//! ```
//! Mask bits beyond the known fields are `InvalidValue` (a newer writer must bump the version).

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
use crate::edit::Edit;

/// Edit blob magic.
pub const EDIT_MAGIC: [u8; 4] = *b"PWED";
/// Current edit blob layout version.
pub const EDIT_VERSION: u16 = 1;

/// Appends the v1 blob for `edit`.
pub fn encode_edit(edit: &Edit, out: &mut Vec<u8>) {
    todo!()
}

/// Decodes exactly one v1 edit blob. Patches are decoded as given (field applicability is the scene's check,
/// not the codec's).
///
/// # Errors
/// As [`super::item::decode_item`]; plus `InvalidValue` for an unknown edit tag or unknown mask bits.
pub fn decode_edit(bytes: &[u8]) -> Result<Edit, CodecError> {
    todo!()
}

/// Appends the edit body only (no header/trailer).
pub fn encode_edit_body(edit: &Edit, out: &mut Vec<u8>) {
    todo!()
}

/// Reads an edit body written by [`encode_edit_body`].
///
/// # Errors
/// As [`decode_edit`] minus header/trailer errors.
pub fn decode_edit_body(reader: &mut Reader<'_>) -> Result<Edit, CodecError> {
    todo!()
}
