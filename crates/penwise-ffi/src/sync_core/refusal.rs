//! Ops the server refused: their effect leaves the replica, `ClockSkew` refusals are re-stamped and retried
//! once, `Duplicate` refusals (the server already has the entity) are dropped, everything else becomes a
//! dead letter ([`SyncCore::dead_letters`]).

use penwise_proto::{AnyOp, OpRejected, OpRejection};

use super::SyncCore;
use super::outcome::{Change, Outcome};
use super::session::count;

impl SyncCore {
    /// Handles the `rejected` list of one `Ack`.
    ///
    /// A refusal for `ClockSkew` means this device's clock ran ahead before the first calibration (the HLC is
    /// monotonic, so every later stamp stayed ahead). The first such refusal on a connection rebases the clock
    /// to the server time (never below a stamp the replica holds), re-stamps every pending op in order and
    /// keeps the ops that were refused only because they referenced a refused one. A second skew refusal before
    /// any `Ack` without refusals is final.
    ///
    /// The replica still shows the effect of every refused op, so it is rebuilt from what remains pending and
    /// the outcome asks for a reconnect: the server's state arrives again and the dead letters are gone.
    pub(super) fn on_refused(&mut self, rejected: &[OpRejected], now_ms: u64, out: &mut Outcome) {
        if rejected.is_empty() {
            self.restamped = false;
            return;
        }
        let retry = !self.restamped && rejected.iter().any(|r| r.reason == OpRejection::ClockSkew);
        let mut dead = Vec::new();
        let mut known = Vec::new();
        for r in rejected {
            match r.reason {
                OpRejection::Duplicate => known.push(r.op_id),
                OpRejection::ClockSkew
                | OpRejection::UnknownPage
                | OpRejection::UnknownNotebook
                | OpRejection::UnknownItem
                    if retry => {}
                _ => dead.push(*r),
            }
        }
        self.outbox.ack(&known);
        self.outbox.reject(&dead);
        out.rejected = count(dead.len());
        if retry {
            self.restamped = true;
            self.restamp_pending(now_ms);
        }
        self.rederive(out);
    }

    /// Rebases the clock onto the corrected server time and re-stamps every pending op in order.
    fn restamp_pending(&mut self, now_ms: u64) {
        let corrected = self.local_now(now_ms);
        self.hlc.rebase(self.remote_high, corrected);
        let ops: Vec<AnyOp> = self
            .outbox
            .pending_ops()
            .into_iter()
            .map(|op| {
                let hlc = self.hlc.send(corrected);
                op.with_hlc(hlc)
            })
            .collect();
        let _ = self.outbox.replace_pending(&ops);
    }

    /// Rebuilds the replica from the pending ops alone, reports every page as changed and asks the host to
    /// reconnect (a fresh `Hello` at cursor 0 brings the server's state back).
    pub(super) fn rederive(&mut self, out: &mut Outcome) {
        let pages = self.page_ids();
        self.rebuild_from_outbox();
        for page in pages {
            out.note(Change { page, item: None });
        }
        out.catalog_changed = true;
        out.reconnect = true;
        self.go_offline();
    }

    /// Replica = the ops not yet acknowledged, nothing else.
    pub(super) fn rebuild_from_outbox(&mut self) {
        let ops = self.outbox.pending_ops();
        self.replica.rebuild_from(&ops);
    }
}
