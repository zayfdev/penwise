//! When a still is re-made.
//!
//! Every trigger lands in [`trigger`], which waits [`DEBOUNCE`] and lets only the newest trigger of a frame
//! fire, so a burst (an editor save, a patch applied in several ops) is one. A fired trigger then
//!
//! - does nothing when a capture that began after the trigger already ran (a new frame is shot once, not by its
//!   first poll and again by its creation trigger);
//! - only marks the frame *stale* when nobody polled it in the last 10 s: nobody is looking, so the next poll
//!   captures before it answers;
//! - otherwise captures, coalescing with any capture already running ([`super::Stills::refresh`]).
//!
//! Sources:
//! - the op log: a frame is added, its route, preset, patch or extent is updated, or a bound stroke arrives for
//!   an element the frame's still does not watch yet (the storage broadcast sees every committed op, from any
//!   device);
//! - the project change feed: a static folder reloaded, or a dev server announced an HMR update or full reload
//!   ([`crate::frames::registry::Registry::subscribe_changes`]) — every known frame of that project.
//!
//! The server's own `FrameDocHeight` writes and its switch to `viewport` are not triggers.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use penwise_core::edit::Edit;
use penwise_core::item::{ItemId, ItemKind};
use penwise_core::marks::BindingMode;
use penwise_proto::{AnyOp, SeqOp};
use tokio::sync::broadcast;

use super::{Inner, Stills};

/// A trigger waits this long for more triggers of the same frame.
pub(super) const DEBOUNCE: Duration = Duration::from_millis(500);

/// What one committed op means for stills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Change {
    /// The frame (new or changed) needs a new still.
    Restill(ItemId),
    /// A stroke is bound to `key` on the frame: it needs a new still unless the current one watches `key`.
    Watch(ItemId, String),
    /// The frame is gone.
    Gone(ItemId),
    /// Nothing to do.
    Ignore,
}

/// Classifies one op. Updates that touch only measurements, position or marks are ignored; so is an extent
/// change when `own_flip` says the server wrote it.
pub(super) fn classify(op: &AnyOp, own_flip: impl Fn(ItemId) -> bool) -> Change {
    let AnyOp::Item(op) = op else {
        return Change::Ignore;
    };
    match &op.edit {
        Edit::Add(item) => match &item.kind {
            ItemKind::Frame(_) => Change::Restill(item.id),
            ItemKind::Stroke(stroke) => match &stroke.binding {
                Some(binding) if binding.mode == BindingMode::Bound => binding
                    .key
                    .clone()
                    .map_or(Change::Ignore, |key| Change::Watch(binding.frame, key)),
                _ => Change::Ignore,
            },
            _ => Change::Ignore,
        },
        Edit::Delete(id) => Change::Gone(*id),
        Edit::Update { id, patch } => {
            let content = patch.frame_route.is_some()
                || patch.frame_preset.is_some()
                || patch.frame_patch.is_some();
            let extent = patch.frame_extent.is_some() && !own_flip(*id);
            if content || extent {
                Change::Restill(*id)
            } else if patch.deleted == Some(true) {
                Change::Gone(*id)
            } else {
                Change::Ignore
            }
        }
    }
}

impl Stills {
    /// Starts following the op log and the projects' change feed. Idempotent. Needs a Tokio runtime.
    pub(crate) fn start(&self) {
        if self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(follow_ops(weak.clone(), self.inner.storage.subscribe()));
        tokio::spawn(follow_projects(
            weak,
            self.inner.projects.subscribe_changes(),
        ));
    }
}

/// Re-makes the frame's still after [`DEBOUNCE`] of quiet. Needs a Tokio runtime.
pub(super) fn trigger(inner: &Arc<Inner>, frame: ItemId) {
    let triggered = Instant::now();
    let generation = inner.next_generation(frame);
    let weak = Arc::downgrade(inner);
    tokio::spawn(async move {
        tokio::time::sleep(DEBOUNCE).await;
        if let Some(inner) = weak.upgrade()
            && inner.owns_generation(frame, generation)
        {
            fire(inner, frame, triggered).await;
        }
    });
}

/// What a trigger that survived the debounce does (see the module docs).
pub(super) async fn fire(inner: Arc<Inner>, frame: ItemId, triggered: Instant) {
    // Something changed: a failure from before it may no longer apply (the dev server is back, the route fixed).
    inner.clear_failure(frame);
    if inner.covered(frame, triggered).is_some() {
        return;
    }
    if !inner.polled_recently(frame) {
        inner.mark_stale(frame);
        return;
    }
    let stills = Stills { inner };
    if let Err(error) = stills.capture_after(frame, triggered).await {
        tracing::warn!(frame = %frame.to_hex(), %error, "frame still not made");
    }
}

async fn follow_ops(inner: Weak<Inner>, mut ops: broadcast::Receiver<Arc<[SeqOp]>>) {
    loop {
        let batch = match ops.recv().await {
            Ok(batch) => Some(batch),
            Err(broadcast::error::RecvError::Lagged(_)) => None,
            Err(broadcast::error::RecvError::Closed) => return,
        };
        let Some(inner) = inner.upgrade() else {
            return;
        };
        let Some(batch) = batch else {
            // Missed ops: every frame we know may have changed.
            let frames = inner.cache().frames();
            for frame in frames {
                trigger(&inner, frame);
            }
            continue;
        };
        for seq_op in batch.iter() {
            match classify(&seq_op.op, |id| inner.flips().remove(&id)) {
                Change::Restill(frame) => trigger(&inner, frame),
                Change::Watch(frame, key) => {
                    let known = inner
                        .cache()
                        .get(frame)
                        .is_some_and(|still| still.rects.contains_key(&key));
                    if !known {
                        trigger(&inner, frame);
                    }
                }
                Change::Gone(frame) => inner.forget(frame),
                Change::Ignore => {}
            }
        }
    }
}

async fn follow_projects(inner: Weak<Inner>, mut changes: broadcast::Receiver<String>) {
    loop {
        let project = match changes.recv().await {
            Ok(project) => Some(project),
            Err(broadcast::error::RecvError::Lagged(_)) => None,
            Err(broadcast::error::RecvError::Closed) => return,
        };
        let Some(inner) = inner.upgrade() else {
            return;
        };
        let frames = match &project {
            Some(project) => inner.cache().frames_of(project),
            None => inner.cache().frames(),
        };
        for frame in frames {
            trigger(&inner, frame);
        }
    }
}
