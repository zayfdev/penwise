//! Frame edit helpers (design harness M4): the ghost's patch, the owner's pick, and the variant frames an
//! agent proposes. Every helper returns the `Edit`s to commit; the caller applies them as one batch, so one
//! commit is one undo step (contracts P2, P4).

use super::{
    FrameExtent, FrameItem, MAX_FRAME_SIDE, MAX_GROUP_LEN, MAX_PATCH_LEN, MAX_VARIANTS,
    VARIANT_GAP_PX,
};
use crate::edit::{Edit, EditError, ItemPatch};
use crate::item::{Affine, Item, ItemId, ItemKind, Layer, LayerSet, Scene, Z_STEP};

/// One variant frame to create next to a base frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantSpec {
    /// The frame's item id, minted by the caller.
    pub id: ItemId,
    /// The label the owner sees on the frame.
    pub label: String,
    /// The variant's P1 patch JSON.
    pub patch: String,
}

/// The frame item `id`, if `id` is a live frame.
#[must_use]
pub fn frame_of(scene: &Scene, id: ItemId) -> Option<&FrameItem> {
    match &scene.get(id)?.kind {
        ItemKind::Frame(frame) => Some(frame),
        _ => None,
    }
}

/// Live frames of variant group `group`, in paint order.
#[must_use]
pub fn group_frames(scene: &Scene, group: &str) -> Vec<ItemId> {
    scene
        .render_order(LayerSet::ALL)
        .into_iter()
        .filter(|id| {
            frame_of(scene, *id).is_some_and(|frame| frame.group.as_deref() == Some(group))
        })
        .collect()
}

/// Edits giving frame `id` `patch` (`None` clears it): one `Field::FramePatch` update, so it syncs by LWW and
/// undoes like any other field. Empty for an unknown/non-frame id or when the patch is unchanged.
///
/// # Errors
/// [`EditError::InvalidValue`] naming `"frame_patch"` when the text exceeds [`MAX_PATCH_LEN`].
pub fn set_patch(scene: &Scene, id: ItemId, patch: Option<String>) -> Result<Vec<Edit>, EditError> {
    let Some(frame) = frame_of(scene, id) else {
        return Ok(Vec::new());
    };
    if patch
        .as_ref()
        .is_some_and(|text| text.len() > MAX_PATCH_LEN)
    {
        return Err(invalid("frame_patch"));
    }
    if frame.patch == patch {
        return Ok(Vec::new());
    }
    Ok(vec![update(
        id,
        ItemPatch {
            frame_patch: Some(patch),
            ..ItemPatch::default()
        },
    )])
}

/// Edits giving frame `id` `extent`: one `Field::FrameExtent` update (LWW). Empty for an unknown/non-frame id
/// or when the extent is unchanged.
#[must_use]
pub fn set_extent(scene: &Scene, id: ItemId, extent: FrameExtent) -> Vec<Edit> {
    let Some(frame) = frame_of(scene, id) else {
        return Vec::new();
    };
    if frame.extent == extent {
        return Vec::new();
    }
    vec![update(
        id,
        ItemPatch {
            frame_extent: Some(extent),
            ..ItemPatch::default()
        },
    )]
}

/// Edits recording the measured document height `doc_h` (CSS px, `0` = unknown) on frame `id`: one
/// `Field::FrameDocHeight` update (LWW), written by the server. Empty for an unknown/non-frame id or when the
/// height is unchanged.
///
/// # Errors
/// [`EditError::InvalidValue`] naming `"frame_doc_h"` when `doc_h` exceeds [`MAX_FRAME_SIDE`].
pub fn set_doc_height(scene: &Scene, id: ItemId, doc_h: u32) -> Result<Vec<Edit>, EditError> {
    let Some(frame) = frame_of(scene, id) else {
        return Ok(Vec::new());
    };
    if doc_h > MAX_FRAME_SIDE {
        return Err(invalid("frame_doc_h"));
    }
    if frame.doc_h == doc_h {
        return Ok(Vec::new());
    }
    Ok(vec![update(
        id,
        ItemPatch {
            frame_doc_h: Some(doc_h),
            ..ItemPatch::default()
        },
    )])
}

/// Edits setting the owner's pick: `chosen` on `id`, and `false` on every other frame of the same group that
/// was chosen, in one batch (one undo step). Choosing is a group decision, so a frame without a group is a
/// no-op; so is an unknown/non-frame id or an unchanged state.
///
/// # Errors
/// [`EditError::UnknownItem`] only if the scene rejects the update.
pub fn set_chosen(scene: &Scene, id: ItemId, chosen: bool) -> Result<Vec<Edit>, EditError> {
    let Some(frame) = frame_of(scene, id) else {
        return Ok(Vec::new());
    };
    let Some(group) = frame.group.as_deref() else {
        return Ok(Vec::new());
    };
    let mut edits = Vec::new();
    if frame.chosen != chosen {
        edits.push(update(
            id,
            ItemPatch {
                frame_chosen: Some(chosen),
                ..ItemPatch::default()
            },
        ));
    }
    if chosen {
        for other in group_frames(scene, group) {
            let sibling = frame_of(scene, other).is_some_and(|frame| frame.chosen);
            if other != id && sibling {
                edits.push(update(
                    other,
                    ItemPatch {
                        frame_chosen: Some(false),
                        ..ItemPatch::default()
                    },
                ));
            }
        }
    }
    Ok(edits)
}

/// The `Add` edits for `variants` beside frame `base`: same project, route, preset, size and extent (and
/// document height hint), first variant
/// at `base.right + 40`, each next one 40 px right of the previous. They join group `group` unchosen; `z` is
/// the first frame's z and each next one is one [`Z_STEP`] higher.
///
/// # Errors
/// [`EditError::InvalidValue`] naming `"frame"` (unknown base frame), `"frame_variant"` (0 or more than
/// [`MAX_VARIANTS`] variants, or a z overflow), `"frame_label"`, `"frame_patch"` or `"frame_group"`.
pub fn variant_frames(
    scene: &Scene,
    base: ItemId,
    variants: &[VariantSpec],
    group: &str,
    z: i64,
) -> Result<Vec<Edit>, EditError> {
    let base_item = scene
        .get(base)
        .filter(|item| matches!(item.kind, ItemKind::Frame(_)))
        .ok_or_else(|| invalid("frame"))?;
    let ItemKind::Frame(base_frame) = &base_item.kind else {
        return Err(invalid("frame"));
    };
    if variants.is_empty() || variants.len() > MAX_VARIANTS {
        return Err(invalid("frame_variant"));
    }
    if group.is_empty() || group.len() > MAX_GROUP_LEN {
        return Err(invalid("frame_group"));
    }
    let mut x = base_item.transform.e + base_frame.page_size()[0] + VARIANT_GAP_PX;
    let mut edits = Vec::with_capacity(variants.len());
    for (index, variant) in variants.iter().enumerate() {
        let frame = FrameItem {
            project: base_frame.project.clone(),
            route: base_frame.route.clone(),
            preset: base_frame.preset,
            css_w: base_frame.css_w,
            css_h: base_frame.css_h,
            patch: Some(variant.patch.clone()),
            label: variant.label.clone(),
            group: Some(group.to_owned()),
            chosen: false,
            extent: base_frame.extent,
            doc_h: base_frame.doc_h,
        };
        frame.check()?;
        edits.push(Edit::Add(Item {
            id: variant.id,
            layer: Layer::Ink,
            z: frame_z(z, index)?,
            transform: Affine::translate(x, base_item.transform.f),
            kind: ItemKind::Frame(frame),
        }));
        x += base_frame.page_size()[0] + VARIANT_GAP_PX;
    }
    Ok(edits)
}

/// `z` advanced by `index` [`Z_STEP`]s.
fn frame_z(z: i64, index: usize) -> Result<i64, EditError> {
    let step = i64::try_from(index)
        .ok()
        .and_then(|i| i.checked_mul(Z_STEP))
        .ok_or_else(|| invalid("frame_variant"))?;
    z.checked_add(step).ok_or_else(|| invalid("frame_variant"))
}

fn update(id: ItemId, patch: ItemPatch) -> Edit {
    Edit::Update { id, patch }
}

fn invalid(field: &'static str) -> EditError {
    EditError::InvalidValue { field }
}

#[cfg(test)]
mod tests;
