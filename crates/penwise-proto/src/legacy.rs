//! Converters from the pre-rewrite data shapes (JSON `StrokeMsg`, `ImageMsg`, text ids `pg_…`/`nb_…`) to core
//! items and ops (design §3.6, §3.7). Used by the server's v3 → v4 migration and by the tablet's one-time
//! import of its local files.
//!
//! **Determinism is the point.** Every id derived here is a pure function of the legacy text, so a migration
//! that runs twice, and a tablet importing the same strokes the server already migrated, produce the *same*
//! `ItemId`/`OpId` and converge by `op_id` dedupe instead of duplicating ink:
//! - notebook / page id: `from_parts(0, sha256(domain ‖ legacy id)[..10])`;
//! - item id: `from_parts(0, sha256("penwise-legacy-item" ‖ page text ‖ 0x00 ‖ row id)[..10])`;
//! - op id: `sha256("penwise-legacy-op" ‖ item id)[..16]`.
//!
//! **Seed.** The row id *is* the stroke's identity (a decimal string; the blob's own 8 seed bytes were written
//! with `parse::<u64>().unwrap_or(0)` and are wrong for negative seeds, so they are never read):
//! `i64` parse reinterpreted as `u64` (two's complement — what the tablet's FFI does), else `u64` parse, else
//! the first 8 bytes (little-endian) of `sha256("penwise-legacy-seed" ‖ row id)`.

use penwise_core::brush::BrushParams;
use penwise_core::edit::{Edit, EditError};
use penwise_core::item::{
    Affine, ImageItem, Item, ItemId, ItemKind, Layer, MAX_STROKE_SAMPLES, StrokeItem,
};
use penwise_core::sample::Sample;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::hlc::Hlc;
use crate::ids::{DeviceId, NotebookId, OpId, PageId};
use crate::op::{AnyOp, Op};

/// Floats per sample in [`LegacyStroke::samples`]: x, y, pressure, tilt, azimuth.
pub const SAMPLE_STRIDE: usize = 5;

/// A legacy record that cannot become an item. Callers quarantine it; nothing is guessed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LegacyError {
    /// No samples.
    #[error("stroke has no samples")]
    NoSamples,
    /// More samples than a stroke family can address.
    #[error("stroke has too many samples")]
    TooManySamples,
    /// `samples.len() != 5 * times.len()`.
    #[error("samples and times disagree")]
    LengthMismatch,
    /// A NaN or infinite float.
    #[error("non-finite value")]
    NonFinite,
    /// `asset` is not a 64-char hex SHA-256.
    #[error("bad asset hash")]
    BadAsset,
    /// The resulting item fails core validation.
    #[error(transparent)]
    Invalid(#[from] EditError),
}

/// One finished stroke as the old tablet/server JSON carried it (`StrokeMsg`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LegacyStroke {
    /// `ink` or `agent`; any other text counts as ink (as the old server's agent filter did).
    #[serde(default = "default_layer")]
    pub layer: String,
    pub preset: u32,
    pub argb: i32,
    pub scale: f32,
    /// Decimal text of the tablet's signed 64-bit seed; the stroke's identity.
    pub seed: String,
    /// Five floats per sample: x, y, pressure, tilt, azimuth.
    pub samples: Vec<f32>,
    /// Microseconds since the stroke started, one per sample.
    pub times: Vec<i32>,
}

/// One placed picture as the old server JSON carried it (`ImageMsg`; its `label` stays server-side).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LegacyImage {
    /// Stable identity of the picture on its page.
    pub id: String,
    /// Content hash (64 lowercase/uppercase hex chars).
    pub asset: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default = "default_layer")]
    pub layer: String,
}

fn default_layer() -> String {
    "ink".to_owned()
}

fn digest(domain: &str, parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// ULID-shaped id with timestamp 0 and 80 hash bits.
fn hashed_ulid(hash: &[u8; 32]) -> u128 {
    hash.iter()
        .take(10)
        .fold(0_u128, |acc, &b| (acc << 8) | u128::from(b))
}

/// Deterministic [`NotebookId`] of a legacy `nb_…` id.
#[must_use]
pub fn legacy_notebook_id(legacy: &str) -> NotebookId {
    NotebookId::from_parts(
        0,
        hashed_ulid(&digest("penwise-legacy-notebook", &[legacy.as_bytes()])),
    )
}

/// Deterministic [`PageId`] of a legacy `pg_…` id.
#[must_use]
pub fn legacy_page_id(legacy: &str) -> PageId {
    PageId::from_parts(
        0,
        hashed_ulid(&digest("penwise-legacy-page", &[legacy.as_bytes()])),
    )
}

/// Deterministic [`ItemId`] of the object `row_id` on legacy page `page_text`.
#[must_use]
pub fn legacy_item_id(page_text: &str, row_id: &str) -> ItemId {
    let hash = digest(
        "penwise-legacy-item",
        &[page_text.as_bytes(), &[0], row_id.as_bytes()],
    );
    ItemId(crate::ids::ItemRef::from_parts(0, hashed_ulid(&hash)).0)
}

/// Deterministic [`OpId`] of the `Add` that imports item `item`.
#[must_use]
pub fn legacy_op_id(item: ItemId) -> OpId {
    let hash = digest("penwise-legacy-op", &[&item.0]);
    let mut id = [0_u8; 16];
    id.copy_from_slice(hash.get(..16).unwrap_or(&[0; 16]));
    OpId(id)
}

/// Brush seed of a legacy row id (see the module docs).
#[must_use]
pub fn legacy_seed(row_id: &str) -> u64 {
    if let Ok(signed) = row_id.parse::<i64>() {
        return signed.cast_unsigned();
    }
    if let Ok(unsigned) = row_id.parse::<u64>() {
        return unsigned;
    }
    let hash = digest("penwise-legacy-seed", &[row_id.as_bytes()]);
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(hash.get(..8).unwrap_or(&[0; 8]));
    u64::from_le_bytes(bytes)
}

fn layer_of(text: &str) -> Layer {
    if text == "agent" {
        Layer::Agent
    } else {
        Layer::Ink
    }
}

/// Core item for a legacy stroke on `page_text`. `z` is the object's stored z (kept verbatim; for strokes
/// without one — a tablet's file order — pass `(index + 1) * Z_STEP`). Brush params follow the tablet's
/// rule ([`BrushParams::for_preset`]), samples are quantized exactly as the tablet's builder did.
///
/// # Errors
/// [`LegacyError`] for an empty/oversized/mismatched/non-finite stroke or one core rejects.
pub fn stroke_to_item(page_text: &str, stroke: &LegacyStroke, z: i64) -> Result<Item, LegacyError> {
    stroke_to_item_as(page_text, &stroke.seed, stroke, z)
}

/// As [`stroke_to_item`], with the item identity taken from `row_id` instead of the stroke's seed text. For
/// page files whose strokes share a seed (the oldest files had none, so several carry seed `0`): the caller
/// makes the identity unique (`"<seed>"`, then `"<seed>#1"`, …) while the brush seed stays the stroke's own.
///
/// # Errors
/// As [`stroke_to_item`].
pub fn stroke_to_item_as(
    page_text: &str,
    row_id: &str,
    stroke: &LegacyStroke,
    z: i64,
) -> Result<Item, LegacyError> {
    let count = stroke.times.len();
    if count == 0 {
        return Err(LegacyError::NoSamples);
    }
    if u32::try_from(count).map_or(true, |n| n > MAX_STROKE_SAMPLES) {
        return Err(LegacyError::TooManySamples);
    }
    if stroke.samples.len() != count * SAMPLE_STRIDE {
        return Err(LegacyError::LengthMismatch);
    }
    if !stroke.samples.iter().all(|v| v.is_finite()) {
        return Err(LegacyError::NonFinite);
    }
    let samples = stroke
        .samples
        .chunks_exact(SAMPLE_STRIDE)
        .zip(&stroke.times)
        .map(|(v, &t)| Sample {
            x: v[0],
            y: v[1],
            pressure: v[2],
            tilt: v[3],
            azimuth: v[4],
            t_us: u32::try_from(t.max(0)).unwrap_or(0),
        })
        .map(Sample::quantize)
        .collect();
    let preset = usize::try_from(stroke.preset).unwrap_or(usize::MAX);
    let params = BrushParams::for_preset(preset, stroke.argb.cast_unsigned(), stroke.scale);
    let item = Item {
        id: legacy_item_id(page_text, row_id),
        layer: layer_of(&stroke.layer),
        z,
        transform: Affine::IDENTITY,
        kind: ItemKind::Stroke(StrokeItem {
            params,
            seed: legacy_seed(&stroke.seed),
            samples,
            trim: None,
            binding: None,
        }),
    };
    item.validate()?;
    Ok(item)
}

/// Core image item for a legacy picture: local box `[0, w] × [0, h]` placed by translating to `(x, y)`.
///
/// # Errors
/// `BadAsset`, `NonFinite`, or a core validation failure (e.g. non-positive size).
pub fn image_to_item(page_text: &str, image: &LegacyImage, z: i64) -> Result<Item, LegacyError> {
    if ![image.x, image.y, image.w, image.h]
        .iter()
        .all(|v| v.is_finite())
    {
        return Err(LegacyError::NonFinite);
    }
    let item = Item {
        id: legacy_item_id(page_text, &image.id),
        layer: layer_of(&image.layer),
        z,
        transform: Affine::translate(image.x, image.y),
        kind: ItemKind::Image(ImageItem {
            asset: parse_asset(&image.asset)?,
            w: image.w,
            h: image.h,
        }),
    };
    item.validate()?;
    Ok(item)
}

fn parse_asset(hex: &str) -> Result<[u8; 32], LegacyError> {
    if hex.len() != 64 || !hex.is_ascii() {
        return Err(LegacyError::BadAsset);
    }
    let mut out = [0_u8; 32];
    for (slot, pair) in out.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
        let text = core::str::from_utf8(pair).map_err(|_| LegacyError::BadAsset)?;
        *slot = u8::from_str_radix(text, 16).map_err(|_| LegacyError::BadAsset)?;
    }
    Ok(out)
}

/// The `Add` op importing `item` onto `page`, with the deterministic [`legacy_op_id`]. `hlc.device` must be
/// `device` (the migrating server's device, or the tablet's).
#[must_use]
pub fn legacy_add_op(page: PageId, device: DeviceId, hlc: Hlc, item: Item) -> AnyOp {
    AnyOp::Item(Box::new(Op {
        op_id: legacy_op_id(item.id),
        device,
        hlc,
        page,
        edit: Edit::Add(item),
    }))
}
