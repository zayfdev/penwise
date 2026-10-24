//! Ephemeral live ink (PLAN "Sync protocol": in-progress samples streamed as `StrokeDelta`, committed as
//! `AddItem` on pen-up). Never logged, never acknowledged; the server may drop them (lag), receivers may
//! miss chunks. The committed `Edit::Add` with the same id is the only source of truth.
//!
//! Owner: slice d (consumer: browser viewer; the tablet is the producer in M4).
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
use penwise_core::geometry::GeometryView;
use penwise_core::sample::{GridSample, Sample};
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
        todo!()
    }

    /// To the core grid form (fields copied).
    #[must_use]
    pub fn to_grid(self) -> GridSample {
        todo!()
    }

    /// Page-px sample (`GridSample::to_sample`), what `StrokeBuilder::push` takes.
    #[must_use]
    pub fn to_sample(self) -> Sample {
        todo!()
    }
}

impl LiveStroke {
    /// Producer device.
    #[must_use]
    pub fn device(&self) -> DeviceId {
        todo!()
    }

    /// Page the stroke is drawn on.
    #[must_use]
    pub fn page(&self) -> PageId {
        todo!()
    }

    /// Live geometry (stable + tentative parts) from the builder.
    #[must_use]
    pub fn geometry(&self) -> GeometryView<'_> {
        todo!()
    }
}

impl LiveStrokes {
    /// No open strokes.
    #[must_use]
    pub fn new() -> Self {
        todo!()
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
        todo!()
    }

    /// Removes a cancelled stroke; true when it was open.
    pub fn cancel(&mut self, stroke: ItemRef) -> bool {
        todo!()
    }

    /// Removes a stroke whose `Edit::Add` arrived (the committed item now renders it); true when it was open.
    pub fn commit(&mut self, stroke: ItemRef) -> bool {
        todo!()
    }

    /// Removes every stroke (after `Resync` or reconnect).
    pub fn clear(&mut self) {
        todo!()
    }

    /// Open strokes on `page`, ascending stroke id.
    #[must_use]
    pub fn on_page(&self, page: PageId) -> Vec<(ItemRef, &LiveStroke)> {
        todo!()
    }
}
