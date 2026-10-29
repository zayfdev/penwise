//! Frames in the browser (design harness C1/M4): a page's frames as JSON, the add/preset/route edits and the
//! owner's variant pick.
//!
//! Ids here are the 26-char ULID text the rest of the viewer uses (`stroke_begin` returns the same form);
//! `rect` is `[x, y, w, h]` in page units. Every edit goes through [`Viewer::record_local`], so it is one undo
//! step and one queued op, like a finished stroke: the caller flushes with `poll_push()`.

use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::item::{
    FrameItem, FramePreset, FrameSize, FrameSpec, IdSource, ItemId, ItemKind, Layer, LayerSet,
    add_frame, set_chosen,
};
use penwise_proto::{ItemRef, PageId};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

use super::{Viewer, parse_id, whole_ms};
use crate::error::WasmError;

/// Body of `add_frame` (`cssW`/`cssH` only matter for `custom`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddFrameRequest {
    project: String,
    route: String,
    preset: String,
    css_w: Option<u32>,
    css_h: Option<u32>,
    x: f32,
    y: f32,
}

#[wasm_bindgen]
impl Viewer {
    /// Live frames of `page` as JSON:
    /// `[{"id","project","route","preset","cssW","cssH","rect":[x,y,w,h],"patch","label","group","chosen"}]`,
    /// in paint order; `patch` is the P1 patch as an object (or `null`). Unknown page → `[]`.
    ///
    /// # Errors
    /// `JsError` for a malformed page id.
    pub fn frames(&self, page: &str) -> Result<String, JsError> {
        Ok(self.frames_json(parse_id(page)?))
    }

    /// Places a frame on `page` from `{project,route,preset,cssW?,cssH?,x,y}`; returns its id text.
    ///
    /// # Errors
    /// `JsError` before `identify`, for a malformed page id/JSON/preset or a rejected value.
    pub fn add_frame(&mut self, page: &str, json: &str, now_ms: f64) -> Result<String, JsError> {
        Ok(self.place_frame(parse_id(page)?, json, whole_ms(now_ms))?)
    }

    /// Switches frame `id` to `preset` at `css_w × css_h` (fixed presets ignore the sizes). An unknown or
    /// non-frame id, or the same size, pushes nothing.
    ///
    /// # Errors
    /// `JsError` for a malformed id, an unknown preset or a bad custom size.
    pub fn set_frame_preset(
        &mut self,
        id: &str,
        preset: &str,
        css_w: u32,
        css_h: u32,
        now_ms: f64,
    ) -> Result<(), JsError> {
        Ok(self.change_preset(parse_item(id)?, preset, css_w, css_h, whole_ms(now_ms))?)
    }

    /// Points frame `id` at `route`. An unknown or non-frame id, or the same route, pushes nothing.
    ///
    /// # Errors
    /// `JsError` for a malformed id or a route that does not start with `/` or is too long.
    pub fn set_frame_route(&mut self, id: &str, route: &str, now_ms: f64) -> Result<(), JsError> {
        Ok(self.change_route(parse_item(id)?, route, whole_ms(now_ms))?)
    }

    /// Marks variant frame `id` as the owner's pick (or un-marks it); choosing one un-chooses the other frames
    /// of its group in the same undo step. An unknown/non-frame id, a frame without a group, or an unchanged
    /// state pushes nothing.
    ///
    /// # Errors
    /// `JsError` for a malformed id before `identify`, or a rejected edit.
    pub fn set_frame_chosen(&mut self, id: &str, chosen: bool, now_ms: f64) -> Result<(), JsError> {
        Ok(self.choose(parse_item(id)?, chosen, whole_ms(now_ms))?)
    }
}

impl Viewer {
    /// [`Viewer::frames`] for a parsed page id.
    #[must_use]
    pub fn frames_json(&self, page: PageId) -> String {
        let Some(view) = self.replica.page(page) else {
            return "[]".to_owned();
        };
        let scene = view.scene();
        let frames: Vec<serde_json::Value> = scene
            .render_order(LayerSet::ALL)
            .into_iter()
            .filter_map(|id| {
                let item = scene.get(id)?;
                let ItemKind::Frame(frame) = &item.kind else {
                    return None;
                };
                let rect = scene.bounds(id)?;
                Some(json!({
                    "id": ItemRef::from(id).to_string(),
                    "project": frame.project,
                    "route": frame.route,
                    "preset": frame.preset.as_str(),
                    "cssW": frame.css_w,
                    "cssH": frame.css_h,
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

    /// [`Viewer::add_frame`] typed: no JS error conversion.
    ///
    /// # Errors
    /// As [`Viewer::add_frame`].
    pub fn place_frame(
        &mut self,
        page: PageId,
        json: &str,
        now_ms: u64,
    ) -> Result<String, WasmError> {
        let request: AddFrameRequest =
            serde_json::from_str(json).map_err(|_| WasmError::BadArgument("frame json"))?;
        let preset =
            FramePreset::parse(&request.preset).ok_or(WasmError::BadArgument("frame preset"))?;
        let spec = FrameSpec::new(
            request.project,
            request.route,
            preset,
            request.css_w,
            request.css_h,
            request.x,
            request.y,
        )?;
        let author = self.author.as_mut().ok_or(WasmError::NotIdentified)?;
        if self.replica.page(page).is_none() {
            return Err(WasmError::UnknownPage);
        }
        let z = self
            .replica
            .page(page)
            .and_then(|p| p.scene().next_z())
            .ok_or(WasmError::BadArgument("page z range exhausted"))?;
        author.mint.set_now(now_ms);
        let id = author.mint.next_id();
        self.record_local(page, &[add_frame(&spec, id, Layer::Ink, z)], now_ms)?;
        Ok(ItemRef::from(id).to_string())
    }

    /// [`Viewer::set_frame_preset`] typed.
    ///
    /// # Errors
    /// As [`Viewer::set_frame_preset`].
    pub fn change_preset(
        &mut self,
        id: ItemId,
        preset: &str,
        css_w: u32,
        css_h: u32,
        now_ms: u64,
    ) -> Result<(), WasmError> {
        let preset = FramePreset::parse(preset).ok_or(WasmError::BadArgument("frame preset"))?;
        let size = FrameSize::resolve(preset, Some(css_w), Some(css_h))?;
        let Some((page, current)) = self.frame(id) else {
            return Ok(());
        };
        if current.size() == size {
            return Ok(());
        }
        let edit = Edit::Update {
            id,
            patch: ItemPatch {
                frame_preset: Some(size),
                ..ItemPatch::default()
            },
        };
        self.record_local(page, &[edit], now_ms)
    }

    /// [`Viewer::set_frame_route`] typed.
    ///
    /// # Errors
    /// As [`Viewer::set_frame_route`].
    pub fn change_route(&mut self, id: ItemId, route: &str, now_ms: u64) -> Result<(), WasmError> {
        let Some((page, current)) = self.frame(id) else {
            return Ok(());
        };
        if current.route == route {
            return Ok(());
        }
        let next = FrameItem {
            route: route.to_owned(),
            ..current
        };
        next.check()?;
        let edit = Edit::Update {
            id,
            patch: ItemPatch {
                frame_route: Some(route.to_owned()),
                ..ItemPatch::default()
            },
        };
        self.record_local(page, &[edit], now_ms)
    }

    /// [`Viewer::set_frame_chosen`] typed.
    ///
    /// # Errors
    /// As [`Viewer::set_frame_chosen`].
    pub fn choose(&mut self, id: ItemId, chosen: bool, now_ms: u64) -> Result<(), WasmError> {
        let Some((page, _)) = self.frame(id) else {
            return Ok(());
        };
        let edits = {
            let view = self.replica.page(page).ok_or(WasmError::UnknownPage)?;
            set_chosen(view.scene(), id, chosen)?
        };
        if edits.is_empty() {
            return Ok(());
        }
        self.record_local(page, &edits, now_ms)
    }

    /// `(page, frame)` of live frame `id`, if any page holds it.
    fn frame(&self, id: ItemId) -> Option<(PageId, FrameItem)> {
        let catalog = self.replica.catalog();
        let pages: Vec<PageId> = catalog
            .notebooks()
            .into_iter()
            .flat_map(|n| catalog.pages(n.id))
            .map(|p| p.id)
            .collect();
        pages.into_iter().find_map(|page| {
            let item = self.replica.page(page)?.scene().get(id)?;
            match &item.kind {
                ItemKind::Frame(frame) => Some((page, frame.clone())),
                _ => None,
            }
        })
    }
}

/// The frame's patch as the P1 object the clients read; `null` when absent or unparseable.
fn patch_json(patch: Option<&str>) -> Option<serde_json::Value> {
    patch.and_then(|text| serde_json::from_str(text).ok())
}

/// Parses an item id from its 26-char ULID text.
pub(super) fn parse_item(text: &str) -> Result<ItemId, WasmError> {
    Ok(ItemId::from(parse_id::<ItemRef>(text)?))
}
