//! Marks on the tablet: frame layouts, bound stroke commits, mark JSON and status (design harness K1).
//!
//! The layout is client-local (`MarksState`), so it is never synced; everything it drives (placement,
//! visibility, hit/lasso/erase positions) comes from `penwise_core::marks`, which is the single source of truth.
//! Edits computed on the effective presentation are mapped back to binding-local data before they are applied,
//! so a moved bound stroke only changes its local offset.

use std::collections::BTreeMap;

use penwise_core::edit::Edit;
use penwise_core::item::{FramePreset, ItemId, LayerSet};
use penwise_core::marks::{
    BindingMode, BoundStroke, DIM_OPACITY, FrameLayoutUpdate, FrameLayouts, FrameRect, MarkStatus,
    MarkView, Presentation, add_bound_stroke, merge_layout, prune_marks, set_mark_status,
};
use penwise_core::sample::Sample;
use serde::Deserialize;
use serde_json::json;

use super::SceneHandle;
use super::tools::StrokeSpec;
use crate::error::SyncError;
use crate::render_floats;

/// Client-local mark state: layouts, the show-done flag and the cached presentation (`rev` bumps on any change
/// that can move, hide or dim a stroke).
#[derive(Debug, Default)]
pub(super) struct MarksState {
    pub(super) layouts: FrameLayouts,
    pub(super) show_done: bool,
    rev: u64,
    /// `(rev, presentation)`: `None` when the page has no marks (render the scene as is).
    cache: Option<(u64, Option<Presentation>)>,
}

impl MarksState {
    /// Drops the cached presentation (scene or layout changed).
    pub(super) fn invalidate(&mut self) {
        self.rev = self.rev.wrapping_add(1);
    }
}

/// Body of `sceneAddBoundStroke`'s binding JSON.
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
    fn update(self) -> Result<FrameLayoutUpdate, SyncError> {
        let preset = match self.preset {
            None => None,
            Some(text) => Some(FramePreset::parse(&text).ok_or(SyncError::BadArgument("preset"))?),
        };
        let rects = self
            .rects
            .into_iter()
            .map(|(key, rect)| (key, rect.map(FrameRect::from_array)))
            .collect();
        Ok(FrameLayoutUpdate {
            preset,
            scroll: self.scroll,
            doc: self.doc,
            viewport: self.viewport,
            state: self.state,
            rects,
        })
    }
}

impl SceneHandle {
    /// Merges one `pw:layout` message into `frame`'s layout; true when the visible output changed (a scroll alone
    /// changes none of a full-length frame's marks, see [`merge_layout`]).
    ///
    /// # Errors
    /// [`SyncError::BadArgument`] for unreadable JSON or an unknown preset.
    pub fn set_frame_layout(&mut self, frame: ItemId, json: &str) -> Result<bool, SyncError> {
        let request: LayoutRequest =
            serde_json::from_str(json).map_err(|_| SyncError::BadArgument("frame layout"))?;
        let update = request.update()?;
        let fallback = self.frame_preset(frame);
        let marks = self.marks.get_mut();
        let change = merge_layout(&self.scene, &mut marks.layouts, frame, fallback, &update);
        if change.moves_marks {
            marks.invalidate();
            self.dirty.mark_all();
        }
        Ok(change.moves_marks)
    }

    /// Drops `frame`'s layout entirely (scroll, document size, state and rects), so the next `set_frame_layout`
    /// starts from nothing: a live frame demoted to its still replaces the live layout with the still's, never
    /// merges into it. True when a layout existed (repaint).
    pub fn clear_frame_layout(&mut self, frame: ItemId) -> bool {
        let state = self.marks.get_mut();
        let existed = state.layouts.get(frame).is_some();
        state.layouts.remove(frame);
        if existed {
            state.invalidate();
            self.dirty.mark_all();
        }
        existed
    }

    /// Replaces `frame`'s layout with the one in `json` in a single step: whatever it had (scroll, document size,
    /// state, rects) is gone, nothing is merged, and no state in between is ever visible or repainted. This is how
    /// a live frame demoted to its still hands over. True when the visible output may have changed (repaint).
    ///
    /// # Errors
    /// [`SyncError::BadArgument`] for unreadable JSON or an unknown preset; the old layout is then kept.
    pub fn replace_frame_layout(&mut self, frame: ItemId, json: &str) -> Result<bool, SyncError> {
        let request: LayoutRequest =
            serde_json::from_str(json).map_err(|_| SyncError::BadArgument("frame layout"))?;
        let update = request.update()?;
        let fallback = self.frame_preset(frame);
        let state = self.marks.get_mut();
        state.layouts.remove(frame);
        state.layouts.merge(frame, fallback, &update);
        state.invalidate();
        self.dirty.mark_all();
        Ok(true)
    }

    /// JSON array of the element keys `frame`'s bound strokes need the bridge to watch.
    #[must_use]
    pub fn frame_keys(&self, frame: ItemId) -> String {
        let keys = {
            let state = self.marks.borrow();
            let view = MarkView::new(&self.scene, &state.layouts, state.show_done);
            view.keys(frame)
        };
        serde_json::Value::Array(keys.into_iter().map(serde_json::Value::String).collect())
            .to_string()
    }

    /// Every live mark as JSON (`rect` in page units, `null` when hidden).
    #[must_use]
    pub fn marks_json(&self) -> String {
        let entries = {
            let state = self.marks.borrow();
            let view = MarkView::new(&self.scene, &state.layouts, state.show_done);
            view.mark_entries()
        };
        let marks: Vec<serde_json::Value> = entries
            .into_iter()
            .map(|entry| {
                json!({
                    "id": entry.id.to_hex(),
                    "frame": entry.frame.to_hex(),
                    "preset": entry.preset.as_str(),
                    "status": entry.status.as_str(),
                    "state": entry.state,
                    "strokes": entry.strokes.iter().map(|id| id.to_hex()).collect::<Vec<_>>(),
                    "rect": entry.rect,
                })
            })
            .collect();
        serde_json::Value::Array(marks).to_string()
    }

    /// Sets the show-done view flag; true when the visible output changed.
    pub fn set_show_done(&mut self, on: bool) -> bool {
        let state = self.marks.get_mut();
        if state.show_done == on {
            return false;
        }
        state.show_done = on;
        state.invalidate();
        self.dirty.mark_all();
        true
    }

    /// Sets mark `id`'s status (`new`/`seen`/`done`/`reopened`) as one undoable, synced edit.
    ///
    /// # Errors
    /// [`SyncError::BadArgument`] for an unknown status; [`SyncError::Edit`] if the scene refuses it.
    pub fn set_mark_status(&mut self, id: ItemId, status: &str) -> Result<Vec<Edit>, SyncError> {
        let status = MarkStatus::parse(status).ok_or(SyncError::BadArgument("mark status"))?;
        let edits = set_mark_status(&self.scene, id, status)?;
        self.local(edits)
    }

    /// Commits a stroke drawn on a frame: samples are **page** coords, `binding_json` is
    /// `{"frame":"<32hex>","mode":"bound"|"area","key":"…"?}`; the binding's preset, state and anchor come from
    /// the current layout of that frame. The stroke is grouped into a mark (or starts one) in the same edit.
    ///
    /// # Errors
    /// [`SyncError::BadArgument`] for unreadable JSON/bad samples; [`SyncError::Edit`] for a rejected binding or
    /// z exhaustion.
    pub fn add_bound_stroke(
        &mut self,
        spec: &StrokeSpec<'_>,
        binding_json: &str,
        now_ms: u64,
    ) -> Result<Vec<Edit>, SyncError> {
        let request: BindingRequest = serde_json::from_str(binding_json)
            .map_err(|_| SyncError::BadArgument("binding json"))?;
        let frame =
            ItemId::from_hex(&request.frame).ok_or(SyncError::BadArgument("binding frame"))?;
        let mode =
            BindingMode::parse(&request.mode).ok_or(SyncError::BadArgument("binding mode"))?;
        let samples = page_samples(spec)?;
        let z = self.next_z()?;
        let mark_id = self.new_item_id(now_ms);
        let bound = BoundStroke {
            id: spec.id,
            mark_id,
            frame,
            mode,
            key: request.key,
            z,
            mark_z: z,
            layer: penwise_core::item::Layer::Ink,
            params: penwise_core::BrushParams::for_preset(spec.preset, spec.argb, spec.scale),
            seed: spec.seed,
            samples,
            now_ms,
        };
        let edits = {
            let state = self.marks.borrow();
            add_bound_stroke(&self.scene, &state.layouts, &bound)?
        };
        self.local(edits)
    }

    /// Render data of `id` in the effective presentation, dimmed when its state is not the frame's.
    #[must_use]
    pub fn item_floats(&self, id: ItemId) -> Option<Vec<f32>> {
        self.with_mark_scene(|scene, dim| {
            let mut floats = render_floats::item_floats(scene, id)?;
            if dim.is_some_and(|dim| dim.contains(&id)) {
                dim_opacity(&mut floats);
            }
            Some(floats)
        })
    }

    /// Live item ids on `layers` of the effective presentation, in paint order.
    #[must_use]
    pub fn render_order(&self, layers: LayerSet) -> Vec<ItemId> {
        self.with_mark_scene(|scene, _| scene.render_order(layers))
    }

    /// The scene the interactive tools edit: the effective presentation without dimmed strokes, plus the
    /// presentation needed to map the resulting edits back. `None` when the page has no marks.
    pub(super) fn interaction(&self) -> Option<Presentation> {
        let state = self.marks.borrow();
        let view = MarkView::new(&self.scene, &state.layouts, state.show_done);
        view.has_marks().then(|| view.presentation())
    }

    /// Runs `f` with the effective scene (presentation when the page has marks) and its dim set.
    pub(super) fn with_mark_scene<T>(
        &self,
        f: impl FnOnce(&penwise_core::item::Scene, Option<&std::collections::BTreeSet<ItemId>>) -> T,
    ) -> T {
        let mut state = self.marks.borrow_mut();
        let rev = state.rev;
        if state
            .cache
            .as_ref()
            .is_none_or(|(cached, _)| *cached != rev)
        {
            let view = MarkView::new(&self.scene, &state.layouts, state.show_done);
            let presentation = view.has_marks().then(|| view.presentation());
            state.cache = Some((rev, presentation));
        }
        match state.cache.as_ref().and_then(|(_, p)| p.as_ref()) {
            Some(presentation) => f(&presentation.scene, Some(&presentation.dim)),
            None => f(&self.scene, None),
        }
    }

    /// Applies `edits` computed on the presentation: localized to binding-local data, plus mark pruning for
    /// the strokes the edits delete.
    pub(super) fn commit_presented(
        &mut self,
        presentation: Option<&Presentation>,
        edits: Vec<Edit>,
    ) -> Result<Vec<Edit>, SyncError> {
        let mut edits = match presentation {
            Some(p) => p.localize(edits),
            None => edits,
        };
        edits.extend(prune_marks(&self.scene, &edits));
        self.local(edits)
    }
}

/// Page-coord samples of `spec`, quantized as the live deltas were.
fn page_samples(spec: &StrokeSpec<'_>) -> Result<Vec<Sample>, SyncError> {
    if spec.samples.len() != spec.times.len() * super::tools::SAMPLE_FLOATS || spec.times.is_empty()
    {
        return Err(SyncError::BadArgument("samples and times disagree"));
    }
    Ok(spec
        .samples
        .chunks_exact(super::tools::SAMPLE_FLOATS)
        .zip(spec.times)
        .map(|(s, &t)| Sample {
            x: s[0],
            y: s[1],
            pressure: s[2],
            tilt: s[3],
            azimuth: s[4],
            t_us: t.cast_unsigned(),
        })
        .collect())
}

/// A render-floats header value as a count (non-finite or negative → 0).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "render-floats headers are small non-negative counts"
)]
fn count(v: f32) -> usize {
    if v.is_finite() && v > 0.0 {
        v as usize
    } else {
        0
    }
}

/// [`count`] as a part-kind code.
fn kind_code(v: f32) -> u32 {
    u32::try_from(count(v)).unwrap_or(u32::MAX)
}

/// Multiplies the style opacity of every part of a render-floats array by [`DIM_OPACITY`].
fn dim_opacity(floats: &mut [f32]) {
    /// Opacity index inside a part's payload, per part kind.
    const OUTLINE: u32 = 0;
    const STAMPS: u32 = 1;
    const EVEN_ODD: u32 = 2;
    const TEXT: u32 = 4;
    let Some(parts) = floats.get(1).map(|v| count(*v)) else {
        return;
    };
    let mut at = 2;
    for _ in 0..parts {
        let (Some(kind), Some(len)) = (floats.get(at), floats.get(at + 1)) else {
            return;
        };
        let (kind, len) = (kind_code(*kind), count(*len));
        let payload = at + 2;
        let index = match kind {
            OUTLINE | STAMPS | EVEN_ODD => Some(4),
            TEXT => Some(12),
            _ => None,
        };
        if let Some(index) = index
            && let Some(value) = floats.get_mut(payload + index)
        {
            *value *= DIM_OPACITY;
        }
        at = payload + len;
    }
}
