//! Live (droppable) strokes: bounded per device, fanned out on the live channel, cancelled when the producer
//! goes away. Nothing here is ever stored or acknowledged.

use penwise_proto::live::{MAX_DELTA_SAMPLES, MAX_LIVE_STROKES_PER_DEVICE};
use penwise_proto::{ServerMsg, StrokeCancel, StrokeDelta};

use super::hub::LiveEvent;
use super::session::Session;
use super::store::SyncStore;

/// Largest accepted `StrokeDelta::brush` (encoded params are ~100 bytes).
pub const MAX_BRUSH_BYTES: usize = 1024;

impl<S: SyncStore> Session<S> {
    /// Relays a delta to the other sessions watching its page. Over-limit deltas are dropped, never fatal.
    pub(super) fn on_delta(&mut self, delta: StrokeDelta) {
        if !(1..=MAX_DELTA_SAMPLES).contains(&delta.samples.len())
            || delta.brush.len() > MAX_BRUSH_BYTES
        {
            tracing::debug!(session = self.id, "over-limit stroke delta dropped");
            return;
        }
        let known = self.open.contains_key(&delta.stroke);
        // A stroke belongs to the page of its first delta; later deltas naming another page are dropped.
        if self
            .open
            .get(&delta.stroke)
            .is_some_and(|&page| page != delta.page)
        {
            tracing::debug!(
                session = self.id,
                "delta for another page than the stroke's; dropped"
            );
            return;
        }
        if delta.start == 0 && !known {
            if self.open.len() >= MAX_LIVE_STROKES_PER_DEVICE {
                tracing::debug!(session = self.id, "too many live strokes; delta dropped");
                return;
            }
            self.open.insert(delta.stroke, delta.page);
        } else if !known {
            return;
        }
        let page = delta.page;
        self.publish(
            Some(page),
            &ServerMsg::StrokeDelta {
                device: self.device,
                delta,
            },
        );
    }

    pub(super) fn on_cancel(&mut self, cancel: StrokeCancel) {
        if self.open.remove(&cancel.stroke).is_some() {
            self.publish(
                Some(cancel.page),
                &ServerMsg::StrokeCancel {
                    device: self.device,
                    cancel,
                },
            );
        }
    }

    /// Forwards a live frame unless it is ours, for another page, or the socket queue is full.
    pub(super) fn on_live_event(&self, event: &LiveEvent) {
        if event.origin == self.id {
            return;
        }
        if event.page.is_some() && event.page != self.watch {
            return;
        }
        if !self.out.try_send_live(event.frame.clone()) {
            tracing::debug!(session = self.id, "slow client; live frame dropped");
        }
    }

    /// The producer is gone: every stroke it left open is cancelled for the others.
    pub(super) fn cancel_open_strokes(&mut self) {
        for (stroke, page) in std::mem::take(&mut self.open) {
            self.publish(
                Some(page),
                &ServerMsg::StrokeCancel {
                    device: self.device,
                    cancel: StrokeCancel { stroke, page },
                },
            );
        }
    }
}
