//! Frame items (design harness C1/M4): the page's frames as JSON, the add/preset/route tools and the owner's
//! variant pick.
//!
//! The JSON is the contract the tablet and the PC window share (see `docs/design-harness-spec.md` C1, P2):
//! `[{"id","project","route","preset","cssW","cssH","extent","docH","boxH","rect":[x,y,w,h],"patch","label",
//! "group","chosen","hex"}]`, ids as 32 lowercase hex (`hex` repeats `id`: the form the HTTP routes take, e.g.
//! `/frames/{hex}/still`), `extent` `"viewport"` or `"full"`, `docH` the server's measured document height
//! (`0` = unknown), `boxH` the box height on the board computed by core from the frame's current layout
//! (`FrameItem::box_size(layout doc)`, the size `Placement` clips with; clients never compute it), `rect` in
//! page units, `patch` the P1 patch as an object (or `null`). Every tool
//! applies its edits to the scene, records them as one undo step and returns them for
//! [`SceneHandle::commit_into`], like the pen tools.

use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::item::{
    FrameExtent, FrameItem, FramePreset, FrameSize, FrameSpec, ItemId, ItemKind, Layer, LayerSet,
    add_frame, set_chosen,
};
use penwise_core::marks::frame_box_size;
use serde::Deserialize;
use serde_json::json;

use super::SceneHandle;
use crate::error::SyncError;

/// Body of `sceneAddFrame` (`cssW`/`cssH` only matter for `custom`; `extent` defaults to `"full"`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddFrameRequest {
    project: String,
    route: String,
    preset: String,
    css_w: Option<u32>,
    css_h: Option<u32>,
    #[serde(default)]
    extent: Option<String>,
    x: f32,
    y: f32,
}

impl SceneHandle {
    /// Every live frame as the C1 JSON array, in paint order (bottom → top).
    #[must_use]
    pub fn frames_json(&self) -> String {
        let state = self.marks.borrow();
        let frames: Vec<serde_json::Value> = self
            .scene
            .render_order(LayerSet::ALL)
            .into_iter()
            .filter_map(|id| {
                let item = self.scene.get(id)?;
                let ItemKind::Frame(frame) = &item.kind else {
                    return None;
                };
                let rect = self.scene.bounds(id)?;
                Some(json!({
                    "id": id.to_hex(),
                    "hex": id.to_hex(),
                    "project": frame.project,
                    "route": frame.route,
                    "preset": frame.preset.as_str(),
                    "cssW": frame.css_w,
                    "cssH": frame.css_h,
                    "extent": frame.extent.as_str(),
                    "docH": frame.doc_h,
                    "boxH": frame_box_size(&self.scene, &state.layouts, id)?[1],
                    "rect": [rect.min_x, rect.min_y, rect.width(), rect.height()],
                    "patch": patch_json(frame.patch.as_deref()),
                    "label": frame.label,
                    "group": frame.group,
                    "chosen": frame.chosen,
                }))
            })
            .collect();
        serde_json::Value::Array(frames).to_string()
    }

    /// Places a frame from the C1 JSON body as one undoable `Add` on the ink layer at the next z.
    ///
    /// # Errors
    /// [`SyncError::BadArgument`] for unreadable JSON or an unknown preset; [`SyncError::Edit`] for a bad
    /// project, route, size or position; z exhaustion.
    pub fn add_frame(&mut self, json: &str, now_ms: u64) -> Result<Vec<Edit>, SyncError> {
        let request: AddFrameRequest =
            serde_json::from_str(json).map_err(|_| SyncError::BadArgument("frame json"))?;
        let preset =
            FramePreset::parse(&request.preset).ok_or(SyncError::BadArgument("frame preset"))?;
        let extent = match request.extent.as_deref() {
            None => FrameExtent::Full,
            Some(text) => FrameExtent::parse(text).ok_or(SyncError::BadArgument("frame extent"))?,
        };
        let spec = FrameSpec::new(
            request.project,
            request.route,
            preset,
            request.css_w,
            request.css_h,
            request.x,
            request.y,
        )?
        .with_extent(extent);
        let id = self.new_item_id(now_ms);
        let z = self.next_z()?;
        self.local(vec![add_frame(&spec, id, Layer::Ink, z)])
    }

    /// Switches frame `id` to `preset` at `css_w × css_h` (ignored, and a no-op if unchanged, for the fixed
    /// presets). An unknown or non-frame id is a no-op.
    ///
    /// # Errors
    /// [`SyncError::BadArgument`] for an unknown preset; [`SyncError::Edit`] for a bad custom size.
    pub fn set_frame_preset(
        &mut self,
        id: ItemId,
        preset: &str,
        css_w: u32,
        css_h: u32,
    ) -> Result<Vec<Edit>, SyncError> {
        let preset = FramePreset::parse(preset).ok_or(SyncError::BadArgument("frame preset"))?;
        let size = resolve_size(preset, css_w, css_h)?;
        let Some(frame) = self.frame(id) else {
            return self.local(Vec::new());
        };
        if frame.size() == size {
            return self.local(Vec::new());
        }
        let edit = Edit::Update {
            id,
            patch: ItemPatch {
                frame_preset: Some(size),
                ..ItemPatch::default()
            },
        };
        self.local(vec![edit])
    }

    /// Points frame `id` at `route`. An unknown or non-frame id, or the same route, is a no-op.
    ///
    /// # Errors
    /// [`SyncError::Edit`] for a route that does not start with `/` or exceeds the length limit.
    pub fn set_frame_route(&mut self, id: ItemId, route: &str) -> Result<Vec<Edit>, SyncError> {
        let Some(frame) = self.frame(id) else {
            return self.local(Vec::new());
        };
        if frame.route == route {
            return self.local(Vec::new());
        }
        let next = FrameItem {
            route: route.to_owned(),
            ..frame.clone()
        };
        next.check()?;
        let edit = Edit::Update {
            id,
            patch: ItemPatch {
                frame_route: Some(route.to_owned()),
                ..ItemPatch::default()
            },
        };
        self.local(vec![edit])
    }

    /// Marks variant frame `id` as the owner's pick (or un-marks it). Choosing one un-chooses the other frames
    /// of its group in the same edit batch, so one commit is one undo step. An unknown/non-frame id, a frame
    /// without a group, or an unchanged state commits nothing.
    ///
    /// # Errors
    /// [`SyncError::Edit`] only if the scene rejects the update.
    pub fn set_frame_chosen(&mut self, id: ItemId, chosen: bool) -> Result<Vec<Edit>, SyncError> {
        let edits = set_chosen(&self.scene, id, chosen)?;
        self.local(edits)
    }

    /// The frame's preset, or `custom` when `id` is not a live frame.
    #[must_use]
    pub(super) fn frame_preset(&self, id: ItemId) -> FramePreset {
        self.frame(id)
            .map_or(FramePreset::Custom, |frame| frame.preset)
    }

    pub(super) fn frame(&self, id: ItemId) -> Option<&FrameItem> {
        match &self.scene.get(id)?.kind {
            ItemKind::Frame(frame) => Some(frame),
            _ => None,
        }
    }
}

/// The frame's patch as the P1 object the clients read; `null` when absent or unparseable (a stored string
/// that fails to parse is a bug the server reports, not a reason to break the frame list).
fn patch_json(patch: Option<&str>) -> Option<serde_json::Value> {
    patch.and_then(|text| serde_json::from_str(text).ok())
}

/// `u32` sizes from JS numbers: fixed presets resolve to their own size, custom sizes must both be positive.
fn resolve_size(preset: FramePreset, css_w: u32, css_h: u32) -> Result<FrameSize, SyncError> {
    Ok(FrameSize::resolve(preset, Some(css_w), Some(css_h))?)
}
