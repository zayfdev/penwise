//! Applying logged ops received from the server, including the adoption of the server's version of legacy
//! creations the tablet imported before it caught up.

use penwise_core::edit::Edit;
use penwise_proto::legacy::legacy_op_id;
use penwise_proto::{AnyOp, NotebookChange, PageChange, ReplicaError, SeqOp};

use super::SyncCore;
use super::outcome::{Change, Outcome};

/// A create whose ids are deterministic functions of legacy text: the server's own migration of the same
/// rows logs it under the same op id but with its own stamps.
fn is_legacy_create(op: &AnyOp) -> bool {
    match op {
        AnyOp::Item(i) => matches!(&i.edit, Edit::Add(item) if i.op_id == legacy_op_id(item.id)),
        AnyOp::Page(p) => {
            matches!(p.change, PageChange::Create { .. }) && p.page.timestamp_ms() == 0
        }
        AnyOp::Notebook(n) => {
            matches!(n.change, NotebookChange::Create { .. }) && n.notebook.timestamp_ms() == 0
        }
    }
}

impl SyncCore {
    /// One `OpsSince` batch: every stamp goes through the clock ([`penwise_proto::HlcClock::observe_logged`],
    /// which adopts even a stamp far ahead of the local clock: the log holds it), every op into the replica.
    /// A `Duplicate` for a legacy create means the tablet imported the row itself first: the server's version
    /// replaces it (pending local ops are re-applied afterwards).
    pub(super) fn on_ops(&mut self, ops: &[SeqOp], upto_seq: u64, now_ms: u64, out: &mut Outcome) {
        let local_now = self.local_now(now_ms);
        let mut adopted = false;
        for op in ops {
            let stamp = op.op.hlc();
            self.remote_high = self.remote_high.max(stamp);
            let _ = self.hlc.observe_logged(stamp, local_now);
            let result = match self.replica.apply_seq(op) {
                Err(ReplicaError::Duplicate) if is_legacy_create(&op.op) => {
                    adopted = true;
                    if let Some(page) = op.op.page() {
                        out.note(Change { page, item: None });
                    }
                    self.replica.adopt_create(&op.op)
                }
                other => other,
            };
            match result {
                Ok(applied) => {
                    out.catalog_changed |= applied.catalog_changed;
                    if let (Some(page), Some(item)) = (applied.page, applied.item) {
                        out.applied += 1;
                        out.note(Change {
                            page,
                            item: Some(item),
                        });
                    }
                }
                Err(_) => out.apply_errors += 1,
            }
        }
        if adopted {
            out.catalog_changed = true;
            for op in self.outbox.pending_ops() {
                let _ = self.replica.apply(&op);
            }
        }
        self.replica.advance_to(upto_seq);
        if self.welcomed && self.replica.head_seq() >= self.welcome_head {
            self.caught_up = true;
        }
    }
}
