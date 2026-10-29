//! Pen strokes on a [`Viewer`]: live deltas while drawing, one committed `Add` on pen-up. Every item is built
//! by core (`StrokeItem`) and stamped/queued by [`Author`]; JS only forwards pointer samples and sends the
//! returned frames.
//!
//! Flow: `stroke_begin` → `stroke_push`… (each returns a `StrokeDelta` frame to send, ≤ 256 samples) →
//! `stroke_end` (applies the `Add` locally, queues it) → send `poll_push()`; the `Ack` re-opens the gate.

use penwise_core::brush::BrushParams;
use penwise_core::codec::stroke::encode_params;
use penwise_core::edit::Edit;
use penwise_core::item::{Affine, IdSource, Item, ItemKind, Layer, StrokeItem};
use penwise_core::sample::Sample;
use penwise_proto::live::MAX_DELTA_SAMPLES;
use penwise_proto::{
    ClientMsg, ItemRef, LiveSample, PageId, StrokeCancel, StrokeDelta, encode_client,
};
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

use super::author::PenStroke;
use super::{Viewer, parse_id, whole_ms};
use crate::error::WasmError;
use crate::pointer::SAMPLE_STRIDE;

#[wasm_bindgen]
impl Viewer {
    /// Opens a pen stroke on `page`: `preset` indexes `Preset::ALL`, `argb` is `0xAARRGGBB`, `scale` the size
    /// multiplier ([`BrushParams::for_preset`]). Returns the stroke's item id.
    ///
    /// # Errors
    /// `JsError` before `identify`, for an unknown page, bad brush or a stroke already open.
    pub fn stroke_begin(
        &mut self,
        page: &str,
        preset: u32,
        argb: u32,
        scale: f32,
        now_ms: f64,
    ) -> Result<String, JsError> {
        Ok(self.begin_stroke(parse_id(page)?, preset, argb, scale, whole_ms(now_ms))?)
    }

    /// Appends samples (`SAMPLE_STRIDE` floats each from `pointer_samples`, one `t_us` each, ≤ 256) and returns
    /// the `StrokeDelta` frame to send. The local preview ([`Viewer::render_live_paths`]) already shows them.
    ///
    /// # Errors
    /// `JsError` without an open stroke, for 0 or > 256 samples, or too many samples in the stroke.
    pub fn stroke_push(&mut self, samples: &[f32], times: &[u32]) -> Result<Vec<u8>, JsError> {
        Ok(self.push_stroke(samples, times)?)
    }

    /// Commits the open stroke: applies the `Add` to the replica and queues it (send `poll_push()` next).
    ///
    /// # Errors
    /// `JsError` without an open stroke or when core rejects the item.
    pub fn stroke_end(&mut self, now_ms: f64) -> Result<(), JsError> {
        Ok(self.end_stroke(whole_ms(now_ms))?)
    }

    /// Abandons the open stroke (`pointercancel`): drops the preview and returns the `StrokeCancel` frame.
    ///
    /// # Errors
    /// `JsError` without an open stroke.
    pub fn stroke_cancel(&mut self) -> Result<Vec<u8>, JsError> {
        Ok(self.cancel_stroke()?)
    }

    /// True while a stroke is open.
    #[must_use]
    pub fn stroke_open(&self) -> bool {
        self.author.as_ref().is_some_and(|a| a.pen.is_some())
    }
}

impl Viewer {
    /// See [`Viewer::stroke_begin`].
    ///
    /// # Errors
    /// [`WasmError`] as documented there.
    pub fn begin_stroke(
        &mut self,
        page: PageId,
        preset: u32,
        argb: u32,
        scale: f32,
        now_ms: u64,
    ) -> Result<String, WasmError> {
        let author = self.author.as_mut().ok_or(WasmError::NotIdentified)?;
        if author.pen.is_some() {
            return Err(WasmError::Stroke("a stroke is already open"));
        }
        if self.replica.page(page).is_none() {
            return Err(WasmError::UnknownPage);
        }
        let params = BrushParams::for_preset(preset as usize, argb, scale);
        if !params.is_valid() {
            return Err(WasmError::BadArgument("brush"));
        }
        let mut brush = Vec::new();
        encode_params(&params, &mut brush);
        author.mint.set_now(now_ms);
        let id = author.mint.next_id();
        let seed = author.mint.next_u64();
        author.pen = Some(PenStroke {
            page,
            id,
            params,
            brush,
            seed,
            samples: Vec::new(),
        });
        Ok(ItemRef::from(id).to_string())
    }

    /// See [`Viewer::stroke_push`].
    ///
    /// # Errors
    /// [`WasmError`] as documented there.
    pub fn push_stroke(&mut self, samples: &[f32], times: &[u32]) -> Result<Vec<u8>, WasmError> {
        let author = self.author.as_mut().ok_or(WasmError::NotIdentified)?;
        let device = author.device;
        let pen = author
            .pen
            .as_mut()
            .ok_or(WasmError::Stroke("no stroke is open"))?;
        let count = (samples.len() / SAMPLE_STRIDE).min(times.len());
        if count == 0 || count > MAX_DELTA_SAMPLES {
            return Err(WasmError::BadArgument("sample count must be 1..=256"));
        }
        let start =
            u32::try_from(pen.samples.len()).map_err(|_| WasmError::BadArgument("stroke"))?;
        let grid: Vec<_> = samples
            .chunks_exact(SAMPLE_STRIDE)
            .zip(times)
            .map(|(s, &t)| {
                Sample {
                    x: s[0],
                    y: s[1],
                    pressure: s[2],
                    tilt: s[3],
                    azimuth: s[4],
                    t_us: t,
                }
                .to_grid()
            })
            .collect();
        pen.samples.extend(grid.iter().map(|g| g.to_sample()));
        let delta = StrokeDelta {
            stroke: ItemRef::from(pen.id),
            page: pen.page,
            brush: pen.brush.clone(),
            seed: pen.seed,
            start,
            samples: grid.into_iter().map(LiveSample::from_grid).collect(),
        };
        let _ = self.live.apply_delta(device, &delta);
        Ok(encode_client(&ClientMsg::StrokeDelta(delta))?)
    }

    /// See [`Viewer::stroke_end`].
    ///
    /// # Errors
    /// [`WasmError`] as documented there.
    pub fn end_stroke(&mut self, now_ms: u64) -> Result<(), WasmError> {
        let author = self.author.as_mut().ok_or(WasmError::NotIdentified)?;
        let pen = author
            .pen
            .take()
            .ok_or(WasmError::Stroke("no stroke is open"))?;
        let scene = self
            .replica
            .page(pen.page)
            .ok_or(WasmError::UnknownPage)?
            .scene();
        let z = scene
            .next_z()
            .ok_or(WasmError::BadArgument("page z range exhausted"))?;
        let item = Item {
            id: pen.id,
            layer: Layer::Ink,
            z,
            transform: Affine::IDENTITY,
            kind: ItemKind::Stroke(StrokeItem {
                params: pen.params,
                seed: pen.seed,
                samples: pen.samples,
                trim: None,
                binding: None,
            }),
        };
        let result = self.record_local(pen.page, &[Edit::Add(item)], now_ms);
        self.live.commit(ItemRef::from(pen.id));
        result
    }

    /// See [`Viewer::stroke_cancel`].
    ///
    /// # Errors
    /// [`WasmError`] as documented there.
    pub fn cancel_stroke(&mut self) -> Result<Vec<u8>, WasmError> {
        let author = self.author.as_mut().ok_or(WasmError::NotIdentified)?;
        let pen = author
            .pen
            .take()
            .ok_or(WasmError::Stroke("no stroke is open"))?;
        let stroke = ItemRef::from(pen.id);
        self.live.cancel(stroke);
        Ok(encode_client(&ClientMsg::StrokeCancel(StrokeCancel {
            stroke,
            page: pen.page,
        }))?)
    }
}
