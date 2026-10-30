//! Client frames that are not ops: live stroke previews, page follow/watch, keepalive.

use penwise_core::brush::BrushParams;
use penwise_core::codec::stroke::encode_params;
use penwise_core::item::ItemId;
use penwise_core::sample::Sample;
use penwise_proto::{
    ClientMsg, FOCUS_PROTOCOL_VERSION, ItemRef, LiveSample, PageId, StrokeCancel, StrokeDelta,
    encode_client,
};

use super::SyncCore;
use crate::error::SyncError;

/// One preview chunk of a pen stroke (design §1.2: droppable, never queued).
#[derive(Debug, Clone, PartialEq)]
pub struct LiveChunk<'a> {
    pub page: PageId,
    /// Id the committed `Add` will carry ([`crate::scene_handle::SceneHandle::new_item_id`]).
    pub stroke: ItemId,
    pub params: &'a BrushParams,
    pub seed: u64,
    /// Index of the first sample of this chunk within the stroke.
    pub start: u32,
    pub samples: &'a [Sample],
}

impl SyncCore {
    /// `StrokeDelta` frame (≤ `MAX_DELTA_SAMPLES` samples; samples are quantized like the committed stroke).
    ///
    /// # Errors
    /// Empty/oversized sample list or an encoding failure.
    pub fn live_delta(&self, chunk: &LiveChunk<'_>) -> Result<Vec<u8>, SyncError> {
        if chunk.samples.is_empty() || chunk.samples.len() > penwise_proto::live::MAX_DELTA_SAMPLES
        {
            return Err(SyncError::BadArgument("live chunk sample count"));
        }
        let mut brush = Vec::new();
        encode_params(chunk.params, &mut brush);
        let samples = chunk
            .samples
            .iter()
            .map(|s| LiveSample::from_grid(s.to_grid()))
            .collect();
        Ok(encode_client(&ClientMsg::StrokeDelta(StrokeDelta {
            stroke: ItemRef::from(chunk.stroke),
            page: chunk.page,
            brush,
            seed: chunk.seed,
            start: chunk.start,
            samples,
        }))?)
    }

    /// `StrokeCancel` frame (pen cancelled / palm rejection).
    ///
    /// # Errors
    /// Encoding failure (never expected).
    pub fn live_cancel(&self, page: PageId, stroke: ItemId) -> Result<Vec<u8>, SyncError> {
        Ok(encode_client(&ClientMsg::StrokeCancel(StrokeCancel {
            stroke: ItemRef::from(stroke),
            page,
        }))?)
    }

    /// `SwitchPage` frame (tell the other devices which page this tablet shows).
    ///
    /// # Errors
    /// Encoding failure (never expected).
    pub fn switch_page(&self, page: PageId) -> Result<Vec<u8>, SyncError> {
        Ok(encode_client(&ClientMsg::SwitchPage { page })?)
    }

    /// True when this connection's `Welcome` reported protocol 2 or newer (`Focus` may be sent).
    #[must_use]
    pub fn server_supports_focus(&self) -> bool {
        self.server_protocol >= FOCUS_PROTOCOL_VERSION
    }

    /// `Focus` frame (ask every other device to show `page` and fit frame `item`; `None` fits the page's
    /// content). `None` when the server's `Welcome` did not report protocol 2: an older server would drop it.
    ///
    /// # Errors
    /// Encoding failure (never expected).
    pub fn focus(&self, page: PageId, item: Option<ItemId>) -> Result<Option<Vec<u8>>, SyncError> {
        if !self.server_supports_focus() {
            return Ok(None);
        }
        Ok(Some(encode_client(&ClientMsg::Focus {
            page,
            item: item.map(ItemRef::from),
        })?))
    }

    /// `Watch` frame (`None` stops watching).
    ///
    /// # Errors
    /// Encoding failure (never expected).
    pub fn watch(&self, page: Option<PageId>) -> Result<Vec<u8>, SyncError> {
        Ok(encode_client(&ClientMsg::Watch { page })?)
    }

    /// `Ping` frame.
    ///
    /// # Errors
    /// Encoding failure (never expected).
    pub fn ping(&self, nonce: u64) -> Result<Vec<u8>, SyncError> {
        Ok(encode_client(&ClientMsg::Ping { nonce })?)
    }
}
