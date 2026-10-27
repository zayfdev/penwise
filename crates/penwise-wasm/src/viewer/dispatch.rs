//! `receive` dispatch: one decoded `ServerMsg` → replica/live-strokes calls + the [`Received`] summary.
//!
//! - `OpsSince` → `Replica::apply_seq` per op (refusals counted in `apply_errors`); every item `Add` also
//!   calls `LiveStrokes::commit` (the preview is replaced by the committed stroke), then the cursor advances
//!   to `upto_seq`.
//! - `SnapshotBegin`/`Chunk`/`End` → the replica's snapshot calls; changed pages and catalog flag collected.
//! - `StrokeDelta`/`StrokeCancel` → `LiveStrokes::apply_delta`/`cancel` (refusals ignored: previews are
//!   droppable). `Resync` → `LiveStrokes::clear`.
//! - Everything else only fills the summary.

use std::collections::BTreeSet;

use penwise_core::edit::Edit;
use penwise_proto::message::OpRejected;
use penwise_proto::{
    AnyOp, ItemRef, OpId, PageId, Scope, SeqOp, ServerMsg, SnapshotChunk, decode_server,
    newest_stamp,
};

use super::{Viewer, author::Author};
use crate::error::WasmError;
use crate::received::{MessageKind, Received, error_name, reject_name};

/// Pages touched by one frame.
#[derive(Default)]
struct Touched {
    changed: BTreeSet<PageId>,
    live: BTreeSet<PageId>,
}

impl Viewer {
    /// Decodes one server frame and applies it.
    ///
    /// # Errors
    /// [`WasmError::Proto`] when the frame does not decode; nothing is applied then.
    pub fn apply_frame(&mut self, frame: &[u8]) -> Result<Received, WasmError> {
        let msg = decode_server(frame)?;
        Ok(self.apply_msg(&msg))
    }

    /// Applies one decoded message.
    pub fn apply_msg(&mut self, msg: &ServerMsg) -> Received {
        let mut out = Received::empty(MessageKind::of(msg));
        let mut touched = Touched::default();
        match msg {
            ServerMsg::Welcome {
                server,
                head_seq,
                current_page,
                server_now_ms,
                ..
            } => {
                if let Some(author) = self.author.as_mut() {
                    author.on_welcome(*server_now_ms);
                }
                out.head_seq = *head_seq;
                out.server = Some(server.to_string());
                out.page = current_page.map(|p| p.to_string());
            }
            ServerMsg::Reject { reason } => out.reject = Some(reject_name(*reason).to_owned()),
            ServerMsg::OpsSince {
                upto_seq,
                ops,
                head_seq,
                more,
                ..
            } => {
                out.head_seq = *head_seq;
                out.more = *more;
                for seq_op in ops {
                    self.apply_logged(seq_op, &mut out, &mut touched);
                }
                self.replica.advance_to(*upto_seq);
                if !*more && self.author.as_mut().is_some_and(Author::take_restored) {
                    self.replay_pending(&mut out, &mut touched);
                }
            }
            ServerMsg::SnapshotBegin { scope, .. } => {
                self.begin_snapshot(*scope, &mut out, &mut touched);
            }
            ServerMsg::SnapshotChunk(chunk) => self.apply_chunk(chunk, &mut out, &mut touched),
            ServerMsg::SnapshotEnd { head_seq } => self.end_snapshot(*head_seq, &mut touched),
            ServerMsg::StrokeDelta { device, delta } => {
                let _ = self.live.apply_delta(*device, delta);
                out.device = Some(device.to_string());
                touched.live.insert(delta.page);
            }
            ServerMsg::StrokeCancel { device, cancel } => {
                self.live.cancel(cancel.stroke);
                out.device = Some(device.to_string());
                touched.live.insert(cancel.page);
            }
            ServerMsg::SwitchPage { page, by } => {
                out.page = Some(page.to_string());
                out.device = Some(by.to_string());
            }
            ServerMsg::Resync { .. } => self.drop_live(&mut touched),
            ServerMsg::Error { code, detail } => {
                out.error = Some(error_name(*code).to_owned());
                out.detail = Some(detail.clone());
            }
            ServerMsg::Ping { nonce } | ServerMsg::Pong { nonce } => out.nonce = *nonce,
            ServerMsg::Ack {
                op_ids, rejected, ..
            } => self.apply_ack(op_ids, rejected, &mut out),
        }
        if !matches!(msg, ServerMsg::Welcome { .. } | ServerMsg::OpsSince { .. }) {
            out.head_seq = self.replica.head_seq();
        }
        out.changed_pages = touched.changed.iter().map(ToString::to_string).collect();
        out.live_pages = touched.live.iter().map(ToString::to_string).collect();
        out
    }

    fn apply_chunk(&mut self, chunk: &SnapshotChunk, out: &mut Received, touched: &mut Touched) {
        if let (Some(author), Some(newest)) = (self.author.as_mut(), newest_stamp(chunk)) {
            author.observe(newest);
        }
        match self.replica.apply_snapshot_chunk(chunk) {
            Ok(applied) => {
                out.catalog_changed |= applied.catalog_changed;
                touched.changed.extend(applied.pages);
            }
            Err(_) => out.apply_errors += 1,
        }
    }

    /// `Ack`: acknowledged ops leave the outbox; refused ones become dead letters and their effects are
    /// rebuilt out of the replica (the caller reconnects so the server's log refills it).
    fn apply_ack(&mut self, op_ids: &[OpId], rejected: &[OpRejected], out: &mut Received) {
        if let Some(author) = self.author.as_mut()
            && author.on_ack(op_ids, rejected) > 0
        {
            self.replica.rebuild_from(&author.pending_ops());
            self.live.clear();
            out.resync = true;
            out.catalog_changed = true;
        }
        out.acked = u32::try_from(op_ids.len()).unwrap_or(u32::MAX);
        out.rejected = u32::try_from(rejected.len()).unwrap_or(u32::MAX);
    }

    fn apply_logged(&mut self, seq_op: &SeqOp, out: &mut Received, touched: &mut Touched) {
        if let Some(author) = self.author.as_mut() {
            author.observe(seq_op.op.hlc());
        }
        match self.replica.apply_seq(seq_op) {
            Ok(applied) => {
                out.catalog_changed |= applied.catalog_changed;
                touched.changed.extend(applied.page);
            }
            Err(_) => out.apply_errors += 1,
        }
        if let AnyOp::Item(op) = &seq_op.op
            && let Edit::Add(item) = &op.edit
            && self.live.commit(ItemRef::from(item.id))
        {
            touched.live.insert(op.page);
        }
    }

    fn begin_snapshot(&mut self, scope: Scope, out: &mut Received, touched: &mut Touched) {
        if let Some(author) = self.author.as_mut() {
            author.set_snapshotting(true);
        }
        out.scope = Some(match scope {
            Scope::All => "all".to_owned(),
            Scope::Catalog => "catalog".to_owned(),
            Scope::Page(page) => page.to_string(),
        });
        self.snapshot_scope = Some(scope);
        if self.replica.begin_snapshot(scope).is_err() {
            out.apply_errors += 1;
            return;
        }
        match scope {
            Scope::All | Scope::Catalog => out.catalog_changed = true,
            Scope::Page(page) => {
                touched.changed.insert(page);
            }
        }
    }

    /// Snapshot done: local ops the server has not acknowledged go back on top (the snapshot replaced what
    /// they had changed), and pushes resume.
    ///
    /// A `Scope::Page` snapshot does not move the cursor (see `Replica::end_snapshot`): catalog ops logged up to
    /// its `head_seq` were never sent to this client.
    fn end_snapshot(&mut self, head_seq: u64, touched: &mut Touched) {
        if let Some(scope) = self.snapshot_scope.take() {
            self.replica.end_snapshot(scope, head_seq);
        }
        let Some(author) = self.author.as_mut() else {
            return;
        };
        author.set_snapshotting(false);
        author.take_restored();
        for op in author.pending_ops() {
            if let Ok(applied) = self.replica.apply(&op) {
                touched.changed.extend(applied.page);
            }
        }
    }

    /// Shows the ops restored from storage on the replica the server just caught up (re-applying one the
    /// server already echoed changes nothing).
    fn replay_pending(&mut self, out: &mut Received, touched: &mut Touched) {
        let Some(author) = self.author.as_ref() else {
            return;
        };
        for op in author.pending_ops() {
            if let Ok(applied) = self.replica.apply(&op) {
                out.catalog_changed |= applied.catalog_changed;
                touched.changed.extend(applied.page);
            }
        }
    }

    /// `Resync`: forget every preview and report the pages that had one.
    fn drop_live(&mut self, touched: &mut Touched) {
        let catalog = self.replica.catalog();
        let pages = catalog
            .notebooks()
            .into_iter()
            .flat_map(|n| catalog.pages(n.id))
            .map(|p| p.id);
        touched
            .live
            .extend(pages.filter(|&p| !self.live.on_page(p).is_empty()));
        self.live.clear();
    }
}
