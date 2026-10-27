//! Ephemeral live ink (PLAN "Sync protocol": in-progress samples streamed as `StrokeDelta`, committed as
//! `AddItem` on pen-up). Never logged, never acknowledged; the server may drop them (lag), receivers may
//! miss chunks. The committed `Edit::Add` with the same id is the only source of truth.
//!
//!
//! **Chunk rule.** A delta carries `start` = index of its first sample within the stroke. A receiver keeps
//! `next` (samples received so far) per stroke and:
//! - `start == 0`: (re)starts the stroke (new [`penwise_core::StrokeBuilder`] from `brush` + `seed`);
//! - `start == next`: pushes the samples;
//! - anything else (gap or overlap): drops the stroke's preview until the next `start == 0` or its commit.
//!
//! Every delta repeats `brush` (encoded params, ~100 bytes) and `seed`, so `start == 0` never depends on an
//! earlier frame. A stroke ends with its `Edit::Add` (same `ItemRef`; [`LiveStrokes::commit`]) or a
//! [`StrokeCancel`] (pen lifted without commit, tool change, or the producer's session ended: the server
//! sends a cancel for each open stroke of a closing session).

use std::collections::BTreeMap;

use penwise_core::StrokeBuilder;
use penwise_core::codec::stroke::decode_params;
use penwise_core::codec::varint::Reader;
use penwise_core::geometry::GeometryView;
use penwise_core::item::MAX_STROKE_SAMPLES;
use penwise_core::sample::{AZIMUTH_GRID_HALF, GridSample, PRESSURE_STEPS, Sample, TILT_GRID_MAX};
use serde::{Deserialize, Serialize};

use crate::ids::{DeviceId, ItemRef, PageId};

/// Most samples in one delta (≈ 2 s of 120 Hz input; producers send every frame, far fewer).
pub const MAX_DELTA_SAMPLES: usize = 256;
/// Most simultaneously open live strokes per device; a delta starting one more is ignored.
pub const MAX_LIVE_STROKES_PER_DEVICE: usize = 4;

/// Wire form of a core [`GridSample`] (the quantization grid the stroke codec stores, so live and committed
/// ink use bit-identical samples).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveSample {
    pub x: i32,
    pub y: i32,
    pub pressure: u16,
    pub tilt: u16,
    pub azimuth: i16,
    pub t_us: u32,
}

/// One chunk of an in-progress stroke (client → server → every other session).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrokeDelta {
    /// Id the committed stroke item will have.
    pub stroke: ItemRef,
    pub page: PageId,
    /// `penwise_core::codec::stroke::encode_params` bytes.
    pub brush: Vec<u8>,
    pub seed: u64,
    /// Index of `samples[0]` within the stroke.
    pub start: u32,
    /// `1..=MAX_DELTA_SAMPLES` samples.
    pub samples: Vec<LiveSample>,
}

/// Abandons a live stroke that will not be committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrokeCancel {
    pub stroke: ItemRef,
    pub page: PageId,
}

/// Why a delta was not applied (receivers ignore it; informational).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LiveError {
    /// `brush` failed `decode_params`.
    #[error("bad brush params")]
    BadBrush,
    /// Empty, oversized, or off-grid samples (`GridSample` ranges).
    #[error("bad samples")]
    BadSamples,
    /// Gap/overlap per the chunk rule; the preview was dropped.
    #[error("out of order delta")]
    OutOfOrder,
    /// Device already has `MAX_LIVE_STROKES_PER_DEVICE` open strokes.
    #[error("too many live strokes")]
    TooMany,
    /// The stroke id is open under another device or page (ids are unique; a second producer is hostile or
    /// buggy). The existing preview is untouched.
    #[error("stroke belongs to another producer")]
    WrongOwner,
}

/// One receiver-side live stroke.
#[derive(Debug, Clone)]
pub struct LiveStroke {
    device: DeviceId,
    page: PageId,
    next: u32,
    builder: StrokeBuilder,
}

/// Every open live stroke a receiver knows, by stroke id.
#[derive(Debug, Clone, Default)]
pub struct LiveStrokes {
    strokes: BTreeMap<ItemRef, LiveStroke>,
}

impl LiveSample {
    /// From the core grid form.
    #[must_use]
    pub fn from_grid(sample: GridSample) -> Self {
        Self {
            x: sample.x,
            y: sample.y,
            pressure: sample.pressure,
            tilt: sample.tilt,
            azimuth: sample.azimuth,
            t_us: sample.t_us,
        }
    }

    /// To the core grid form (fields copied).
    #[must_use]
    pub fn to_grid(self) -> GridSample {
        GridSample {
            x: self.x,
            y: self.y,
            pressure: self.pressure,
            tilt: self.tilt,
            azimuth: self.azimuth,
            t_us: self.t_us,
        }
    }

    /// Page-px sample (`GridSample::to_sample`), what `StrokeBuilder::push` takes.
    #[must_use]
    pub fn to_sample(self) -> Sample {
        self.to_grid().to_sample()
    }

    /// True when every field lies in its `GridSample` range.
    fn on_grid(self) -> bool {
        const POSITION_GRID_LIMIT: i32 = 1 << 24;
        self.x.abs() <= POSITION_GRID_LIMIT
            && self.y.abs() <= POSITION_GRID_LIMIT
            && f32::from(self.pressure) <= PRESSURE_STEPS
            && self.tilt <= TILT_GRID_MAX
            && (1 - AZIMUTH_GRID_HALF..=AZIMUTH_GRID_HALF).contains(&self.azimuth)
    }
}

impl LiveStroke {
    /// Producer device.
    #[must_use]
    pub fn device(&self) -> DeviceId {
        self.device
    }

    /// Page the stroke is drawn on.
    #[must_use]
    pub fn page(&self) -> PageId {
        self.page
    }

    /// Live geometry (stable + tentative parts) from the builder.
    #[must_use]
    pub fn geometry(&self) -> GeometryView<'_> {
        self.builder.geometry()
    }

    fn push(&mut self, samples: &[LiveSample]) {
        let batch: Vec<Sample> = samples.iter().map(|s| s.to_sample()).collect();
        self.builder.push(&batch);
        self.next = self
            .next
            .saturating_add(u32::try_from(samples.len()).unwrap_or(u32::MAX));
    }
}

impl LiveStrokes {
    /// No open strokes.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies a delta from `device` per the module chunk rule. Returns `Ok(true)` when the preview changed.
    ///
    /// # Errors
    /// [`LiveError`]; on `OutOfOrder` the stroke's preview is removed, otherwise state is unchanged.
    pub fn apply_delta(
        &mut self,
        device: DeviceId,
        delta: &StrokeDelta,
    ) -> Result<bool, LiveError> {
        let count = delta.samples.len();
        if count == 0 || count > MAX_DELTA_SAMPLES || !delta.samples.iter().all(|s| s.on_grid()) {
            return Err(LiveError::BadSamples);
        }
        if let Some(open) = self.strokes.get(&delta.stroke)
            && (open.device != device || open.page != delta.page)
        {
            return Err(LiveError::WrongOwner);
        }
        if delta.start == 0 {
            return self.start(device, delta);
        }
        let Some(open) = self.strokes.get_mut(&delta.stroke) else {
            return Err(LiveError::OutOfOrder);
        };
        let total = u64::from(open.next) + count as u64;
        if delta.start != open.next || total > u64::from(MAX_STROKE_SAMPLES) {
            self.strokes.remove(&delta.stroke);
            return Err(LiveError::OutOfOrder);
        }
        open.push(&delta.samples);
        Ok(true)
    }

    /// `start == 0`: (re)starts the stroke from the delta's brush and seed.
    fn start(&mut self, device: DeviceId, delta: &StrokeDelta) -> Result<bool, LiveError> {
        let mut reader = Reader::new(&delta.brush);
        let params = decode_params(&mut reader).map_err(|_| LiveError::BadBrush)?;
        if reader.remaining() != 0 || !params.is_valid() {
            return Err(LiveError::BadBrush);
        }
        if !self.strokes.contains_key(&delta.stroke) {
            let open = self.strokes.values().filter(|s| s.device == device).count();
            if open >= MAX_LIVE_STROKES_PER_DEVICE {
                return Err(LiveError::TooMany);
            }
        }
        let mut stroke = LiveStroke {
            device,
            page: delta.page,
            next: 0,
            builder: StrokeBuilder::new(&params, delta.seed),
        };
        stroke.push(&delta.samples);
        self.strokes.insert(delta.stroke, stroke);
        Ok(true)
    }

    /// Removes a cancelled stroke; true when it was open.
    pub fn cancel(&mut self, stroke: ItemRef) -> bool {
        self.strokes.remove(&stroke).is_some()
    }

    /// Removes a stroke whose `Edit::Add` arrived (the committed item now renders it); true when it was open.
    pub fn commit(&mut self, stroke: ItemRef) -> bool {
        self.strokes.remove(&stroke).is_some()
    }

    /// Removes every stroke (after `Resync` or reconnect).
    pub fn clear(&mut self) {
        self.strokes.clear();
    }

    /// Open strokes on `page`, ascending stroke id.
    #[must_use]
    pub fn on_page(&self, page: PageId) -> Vec<(ItemRef, &LiveStroke)> {
        self.strokes
            .iter()
            .filter(|(_, s)| s.page == page)
            .map(|(&id, s)| (id, s))
            .collect()
    }
}
