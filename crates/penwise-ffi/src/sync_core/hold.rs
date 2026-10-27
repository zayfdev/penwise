//! A different server answered the `Hello` (the pinned id changed): the replica holds acked ink only that the
//! old server ever saw, so nothing is dropped until the user decides.

use penwise_proto::{ServerId, carry::carry_ops};

use super::SyncCore;
use super::outcome::{Change, Outcome};
use super::session::count;
use crate::error::SyncError;

/// `Outcome::kind` of the result of [`SyncCore::resolve_server_change`] (not a server message).
pub const KIND_RESOLVE: u8 = 255;

impl SyncCore {
    /// Starts holding for `server` (see module docs): welcomed state is withdrawn, nothing is pushed or applied.
    pub(super) fn hold(&mut self, server: ServerId) {
        self.held = Some(server);
        self.welcomed = false;
        self.caught_up = false;
    }

    /// True between a `Welcome` of a different server and [`SyncCore::resolve_server_change`].
    #[must_use]
    pub fn server_change_pending(&self) -> bool {
        self.held.is_some()
    }

    /// Settles a held server change and pins the new server.
    ///
    /// `keep = true`: every live notebook, page and item of the replica becomes a pending `Create`/`Add`
    /// for the new server (deterministic op ids, so repeating this is idempotent there); pending ops are folded
    /// into them. `keep = false`: the replica is rebuilt from the pending ops alone (what the old server held
    /// is dropped). Either way the outcome asks for a reconnect, reports every page as changed and its
    /// `apply_errors` counts carried ops too large to queue.
    ///
    /// # Errors
    /// `BadArgument` when no server change is pending.
    pub fn resolve_server_change(&mut self, keep: bool, now_ms: u64) -> Result<Outcome, SyncError> {
        let server = self
            .held
            .take()
            .ok_or(SyncError::BadArgument("no server change pending"))?;
        let mut out = Outcome {
            kind: super::hold::KIND_RESOLVE,
            ..Outcome::default()
        };
        let pages = self.page_ids();
        if keep {
            let corrected = self.local_now(now_ms);
            let ops = carry_ops(&self.replica, self.device, || self.hlc.send(corrected));
            out.apply_errors = count(self.outbox.replace_pending(&ops));
            self.replica.rebuild_from(&ops);
        } else {
            self.rebuild_from_outbox();
        }
        self.pinned = Some(server);
        self.go_offline();
        self.revision += 1;
        for page in pages {
            out.note(Change { page, item: None });
        }
        out.catalog_changed = true;
        out.reconnect = true;
        out.head_seq = self.replica.head_seq();
        Ok(out)
    }
}
