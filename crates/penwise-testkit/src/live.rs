//! Live (droppable) stroke preview helpers.

use penwise_core::brush::BrushParams;
use penwise_core::codec::stroke::encode_params;
use penwise_core::item::ItemId;
use penwise_core::sample::Sample;
use penwise_proto::{ClientMsg, LiveSample, PageId, StrokeCancel, StrokeDelta};

use crate::TestDevice;

/// A `StrokeDelta` for `points` starting at sample index `start` (brush = preset 0, black).
#[must_use]
pub fn delta(
    page: PageId,
    stroke: ItemId,
    seed: u64,
    start: u32,
    points: &[[f32; 2]],
) -> StrokeDelta {
    let mut brush = Vec::new();
    encode_params(&BrushParams::for_preset(0, 0xFF00_0000, 1.0), &mut brush);
    let samples = points
        .iter()
        .zip(0_u32..)
        .map(|(p, i)| {
            let s = Sample {
                x: p[0],
                y: p[1],
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: (start + i) * 8_333,
            };
            LiveSample::from_grid(s.to_grid())
        })
        .collect();
    StrokeDelta {
        stroke: stroke.into(),
        page,
        brush,
        seed,
        start,
        samples,
    }
}

impl TestDevice {
    /// Sends one live delta. Errors when offline: previews are never queued (only ops are).
    ///
    /// # Errors
    /// Offline or transport error text.
    pub async fn send_live(
        &mut self,
        page: PageId,
        stroke: ItemId,
        start: u32,
        points: &[[f32; 2]],
    ) -> Result<(), String> {
        let d = delta(page, stroke, 7, start, points);
        self.send(&ClientMsg::StrokeDelta(d)).await
    }

    /// Cancels a live stroke.
    ///
    /// # Errors
    /// Offline or transport error text.
    pub async fn cancel_live(&mut self, page: PageId, stroke: ItemId) -> Result<(), String> {
        self.send(&ClientMsg::StrokeCancel(StrokeCancel {
            stroke: stroke.into(),
            page,
        }))
        .await
    }

    /// `Watch{page}`.
    ///
    /// # Errors
    /// Offline or transport error text.
    pub async fn watch(&mut self, page: Option<PageId>) -> Result<(), String> {
        self.send(&ClientMsg::Watch { page }).await
    }
}
