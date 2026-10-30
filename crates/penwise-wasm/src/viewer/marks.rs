//! Marks in the browser (design harness K1): frame layouts, the show-done flag, bound stroke commits, mark
//! JSON and status changes. Placement, visibility and grouping live in `penwise_core::marks`; this module only
//! converts JSON and forwards to core (binding glue, no rules).

use std::collections::BTreeMap;

use penwise_core::item::FramePreset;
use penwise_core::marks::{
    BindingMode, BoundStroke, FrameLayoutUpdate, FrameRect, MarkStatus, MarkView, add_bound_stroke,
    set_mark_status,
};
use penwise_proto::{ItemRef, PageId};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

use super::frames::parse_item;
use super::{Viewer, whole_ms};
use crate::error::WasmError;

/// Body of `stroke_end_bound`'s binding JSON.
#[derive(Debug, Deserialize)]
struct BindingRequest {
    frame: String,
    mode: String,
    #[serde(default)]
    key: Option<String>,
}

/// One `pw:layout` message.
#[derive(Debug, Default, Deserialize)]
struct LayoutRequest {
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    scroll: Option<[f32; 2]>,
    #[serde(default)]
    doc: Option<[f32; 2]>,
    #[serde(default)]
    viewport: Option<[f32; 2]>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    rects: BTreeMap<String, Option<[f32; 4]>>,
}

impl LayoutRequest {
    fn update(self) -> Result<FrameLayoutUpdate, WasmError> {
        let preset = match self.preset {
            None => None,
            Some(text) => Some(FramePreset::parse(&text).ok_or(WasmError::BadArgument("preset"))?),
        };
        Ok(FrameLayoutUpdate {
            preset,
            scroll: self.scroll,
            doc: self.doc,
            viewport: self.viewport,
            state: self.state,
            rects: self
                .rects
                .into_iter()
                .map(|(key, rect)| (key, rect.map(FrameRect::from_array)))
                .collect(),
        })
    }
}

#[wasm_bindgen]
impl Viewer {
    /// Merges one `pw:layout` message into `frame`'s layout; true when the visible output changed (repaint).
    ///
    /// # Errors
    /// `JsError` for a malformed frame id, unreadable JSON or an unknown preset.
    pub fn set_frame_layout(&mut self, frame: &str, json: &str) -> Result<bool, JsError> {
        let frame = parse_item(frame)?;
        let request: LayoutRequest =
            serde_json::from_str(json).map_err(|_| WasmError::BadArgument("frame layout"))?;
        let update = request.update()?;
        let preset = self.frame_preset(frame).unwrap_or(FramePreset::Custom);
        Ok(self.layouts.merge(frame, preset, &update))
    }

    /// Drops `frame`'s layout entirely (scroll, document size, state and rects), so the next `set_frame_layout`
    /// starts from nothing: a live frame demoted to its still replaces the live layout with the still's, never
    /// merges into it. True when a layout existed (repaint).
    ///
    /// # Errors
    /// `JsError` for a malformed frame id.
    pub fn clear_frame_layout(&mut self, frame: &str) -> Result<bool, JsError> {
        let frame = parse_item(frame)?;
        let existed = self.layouts.get(frame).is_some();
        self.layouts.remove(frame);
        Ok(existed)
    }

    /// Element keys of `frame`'s bound strokes as a JSON array (what to send with `pw:watch`).
    ///
    /// # Errors
    /// `JsError` for a malformed frame id.
    pub fn frame_keys(&self, frame: &str) -> Result<String, JsError> {
        let frame = parse_item(frame)?;
        let mut keys = Vec::new();
        for page in self.all_pages() {
            if let Some(replica) = self.replica.page(page) {
                keys = MarkView::new(replica.scene(), &self.layouts, self.show_done).keys(frame);
                if !keys.is_empty() {
                    break;
                }
            }
        }
        Ok(
            serde_json::Value::Array(keys.into_iter().map(serde_json::Value::String).collect())
                .to_string(),
        )
    }

    /// Live marks of `page` as JSON: `[{"id","frame","preset","status","state","strokes","rect"}]`
    /// (26-char ULID ids, `rect` in page units, `null` when hidden). Unknown page → `[]`.
    ///
    /// # Errors
    /// `JsError` for a malformed page id.
    pub fn marks(&self, page: &str) -> Result<String, JsError> {
        let page: PageId = super::parse_id(page)?;
        let Some(replica) = self.replica.page(page) else {
            return Ok("[]".to_owned());
        };
        let view = MarkView::new(replica.scene(), &self.layouts, self.show_done);
        let marks: Vec<serde_json::Value> = view
            .mark_entries()
            .into_iter()
            .map(|entry| {
                json!({
                    "id": ItemRef::from(entry.id).to_string(),
                    "frame": ItemRef::from(entry.frame).to_string(),
                    "preset": entry.preset.as_str(),
                    "status": entry.status.as_str(),
                    "state": entry.state,
                    "strokes": entry
                        .strokes
                        .iter()
                        .map(|id| ItemRef::from(*id).to_string())
                        .collect::<Vec<_>>(),
                    "rect": entry.rect,
                })
            })
            .collect();
        Ok(serde_json::Value::Array(marks).to_string())
    }

    /// Sets mark `id`'s status (`new`/`seen`/`done`/`reopened`): one undoable edit, queued in the outbox.
    ///
    /// # Errors
    /// `JsError` before `identify`, for a malformed id/status or an unknown mark.
    pub fn set_mark_status(&mut self, id: &str, status: &str, now_ms: f64) -> Result<(), JsError> {
        let id = parse_item(id)?;
        let status = MarkStatus::parse(status).ok_or(WasmError::BadArgument("mark status"))?;
        Ok(self.step_mark_status(id, status, whole_ms(now_ms))?)
    }

    /// The local show-done flag; true when the visible output changed.
    pub fn set_show_done(&mut self, on: bool) -> bool {
        if self.show_done == on {
            return false;
        }
        self.show_done = on;
        true
    }

    /// Commits the open stroke against a frame binding (see `stroke_end` for free canvas strokes).
    ///
    /// # Errors
    /// `JsError` without an open stroke, for unreadable binding JSON or when core rejects the item.
    pub fn stroke_end_bound(&mut self, now_ms: f64, binding_json: &str) -> Result<(), JsError> {
        Ok(self.end_stroke_bound(whole_ms(now_ms), binding_json)?)
    }
}

impl Viewer {
    /// The pages the replica knows, for item → page lookups.
    fn all_pages(&self) -> Vec<PageId> {
        let catalog = self.replica.catalog();
        catalog
            .notebooks()
            .iter()
            .flat_map(|notebook| catalog.pages(notebook.id))
            .map(|page| page.id)
            .collect()
    }

    /// The preset of live frame item `frame`.
    fn frame_preset(&self, frame: penwise_core::item::ItemId) -> Option<FramePreset> {
        self.all_pages().into_iter().find_map(|page| {
            let item = self.replica.page(page)?.scene().get(frame)?;
            match &item.kind {
                penwise_core::item::ItemKind::Frame(frame) => Some(frame.preset),
                _ => None,
            }
        })
    }

    /// The page holding live item `id`.
    fn page_of(&self, id: penwise_core::item::ItemId) -> Option<PageId> {
        self.all_pages().into_iter().find(|page| {
            self.replica
                .page(*page)
                .is_some_and(|p| p.scene().get(id).is_some())
        })
    }

    /// See [`Viewer::set_mark_status`].
    ///
    /// # Errors
    /// [`WasmError`] as documented there.
    pub fn step_mark_status(
        &mut self,
        id: penwise_core::item::ItemId,
        status: MarkStatus,
        now_ms: u64,
    ) -> Result<(), WasmError> {
        let page = self.page_of(id).ok_or(WasmError::UnknownPage)?;
        let scene = self
            .replica
            .page(page)
            .ok_or(WasmError::UnknownPage)?
            .scene();
        let edits = set_mark_status(scene, id, status)?;
        self.record_local(page, &edits, now_ms)
    }

    /// See [`Viewer::stroke_end_bound`].
    ///
    /// # Errors
    /// [`WasmError`] as documented there.
    pub fn end_stroke_bound(&mut self, now_ms: u64, binding_json: &str) -> Result<(), WasmError> {
        let request: BindingRequest = serde_json::from_str(binding_json)
            .map_err(|_| WasmError::BadArgument("binding json"))?;
        // Binding JSON carries the same 26-char ULID text as the rest of the wasm API.
        let frame = parse_item(&request.frame)?;
        let mode =
            BindingMode::parse(&request.mode).ok_or(WasmError::BadArgument("binding mode"))?;
        let author = self.author.as_mut().ok_or(WasmError::NotIdentified)?;
        let pen = author
            .pen
            .take()
            .ok_or(WasmError::Stroke("no stroke is open"))?;
        author.mint.set_now(now_ms);
        let mark_id = penwise_core::item::IdSource::next_id(&mut author.mint);
        let page = pen.page;
        let scene = self
            .replica
            .page(page)
            .ok_or(WasmError::UnknownPage)?
            .scene();
        let z = scene
            .next_z()
            .ok_or(WasmError::BadArgument("page z range exhausted"))?;
        let bound = BoundStroke {
            id: pen.id,
            mark_id,
            frame,
            mode,
            key: request.key,
            z,
            mark_z: z,
            layer: penwise_core::item::Layer::Ink,
            params: pen.params,
            seed: pen.seed,
            samples: pen.samples,
            now_ms,
        };
        let edits = add_bound_stroke(scene, &self.layouts, &bound)?;
        let result = self.record_local(page, &edits, now_ms);
        self.live.commit(ItemRef::from(pen.id));
        result
    }
}
