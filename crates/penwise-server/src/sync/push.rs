//! `PushOps` → storage → `Ack`.

use std::collections::HashMap;

use penwise_core::edit::Edit;
use penwise_proto::{AnyOp, ItemRef, OpId, OpRejected, ServerMsg};

use super::outbound::Stop;
use super::session::Session;
use super::store::{AppendFailure, SyncStore};

impl<S: SyncStore> Session<S> {
    /// One frame = one storage transaction (one gesture: never partially applied). A storage failure ends the session (nothing was logged; the
    /// client re-pushes its outbox after reconnecting).
    pub(super) async fn on_push(&mut self, ops: Vec<AnyOp>) -> Result<(), Stop> {
        let order: Vec<OpId> = ops.iter().map(AnyOp::op_id).collect();
        let added: HashMap<OpId, ItemRef> = ops
            .iter()
            .filter_map(|op| match op {
                AnyOp::Item(item) if matches!(item.edit, Edit::Add(_)) => {
                    op.item().map(|stroke| (item.op_id, stroke))
                }
                _ => None,
            })
            .collect();
        let result = self.hub.store.append_ops(self.device, ops).await;
        let (op_ids, seqs, rejected) = match result {
            Ok(seqs) if seqs.len() == order.len() => {
                for op_id in &order {
                    if let Some(stroke) = added.get(op_id) {
                        self.open.remove(stroke);
                    }
                }
                (order, seqs, Vec::new())
            }
            Ok(_) => {
                return Err(Stop::internal(
                    &"storage answered a different number of ops",
                ));
            }
            Err(AppendFailure::Store(error)) => return Err(Stop::internal(&error)),
            // All-or-nothing (a frame is one gesture): nothing was logged. Refused ops are dead-lettered by
            // the client's outbox; the rest stay pending and are re-pushed in a new frame.
            Err(AppendFailure::Rejected(list)) => {
                let rejected = list
                    .into_iter()
                    .map(|(op_id, reason)| OpRejected { op_id, reason })
                    .collect();
                (Vec::new(), Vec::new(), rejected)
            }
        };
        self.out
            .send(&ServerMsg::Ack {
                op_ids,
                seqs,
                rejected,
            })
            .await
    }
}
