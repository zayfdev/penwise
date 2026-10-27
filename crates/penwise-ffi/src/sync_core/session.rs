//! The `/sync` session half of [`SyncCore`]: `Hello`, frame dispatch, outbox flushing.

use penwise_core::edit::Field;
use penwise_core::item::ItemId;
use penwise_proto::{
    AuthToken, ClientKind, ClientMsg, FieldStamps, Hlc, PROTOCOL_VERSION, Replica, Scope,
    ServerMsg, SnapshotChunk, decode_server, encode_client,
};

use super::SyncCore;
use super::outcome::{Change, Outcome, error_code, kind_code, reject_code};
use crate::error::SyncError;

impl SyncCore {
    /// The `Hello` frame for a new connection (cursor = the replica's head). `now_ms` is the local wall clock
    /// right now; the `Welcome` calibrates the clock offset against it.
    ///
    /// # Errors
    /// Frame encoding failure (never expected).
    pub fn hello(
        &mut self,
        token: &str,
        kind: ClientKind,
        now_ms: u64,
    ) -> Result<Vec<u8>, SyncError> {
        self.hello_at = now_ms;
        self.welcomed = false;
        self.inflight = false;
        self.snapshotting = false;
        Ok(encode_client(&ClientMsg::Hello {
            protocol_version: PROTOCOL_VERSION,
            device: self.device,
            token: AuthToken(token.to_owned()),
            last_seen_seq: self.replica.head_seq(),
            client: kind,
        })?)
    }

    /// Decodes and applies one server frame. Apply failures of single ops are counted in the outcome, never
    /// fatal (one bad op must not wedge sync).
    ///
    /// # Errors
    /// The frame does not decode (drop it and log).
    pub fn receive(&mut self, frame: &[u8], now_ms: u64) -> Result<Outcome, SyncError> {
        let msg = decode_server(frame)?;
        let mut out = Outcome {
            kind: kind_code(&msg),
            ..Outcome::default()
        };
        match msg {
            ServerMsg::Welcome {
                server,
                server_now_ms,
                current_page,
                ..
            } => {
                self.on_welcome(server, server_now_ms, now_ms, &mut out);
                out.page = current_page;
            }
            ServerMsg::Reject { reason } => out.code = reject_code(reason),
            ServerMsg::Ack {
                op_ids, rejected, ..
            } => {
                self.outbox.ack(&op_ids);
                self.outbox.reject(&rejected);
                self.inflight = false;
                out.acked = count(op_ids.len());
                out.rejected = count(rejected.len());
            }
            ServerMsg::OpsSince { upto_seq, ops, .. } => {
                for op in &ops {
                    let _ = self.hlc.recv(op.op.hlc(), self.local_now(now_ms));
                    match self.replica.apply_seq(op) {
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
                self.replica.advance_to(upto_seq);
            }
            ServerMsg::SnapshotBegin { scope, .. } => self.on_snapshot_begin(scope, &mut out),
            ServerMsg::SnapshotChunk(chunk) => self.on_chunk(&chunk, now_ms, &mut out),
            ServerMsg::SnapshotEnd { head_seq } => self.on_snapshot_end(head_seq, &mut out),
            ServerMsg::SwitchPage { page, .. } => out.page = Some(page),
            ServerMsg::Error { code, .. } => out.code = error_code(code),
            ServerMsg::Ping { nonce } => {
                out.replies.push(encode_client(&ClientMsg::Pong { nonce })?);
            }
            ServerMsg::StrokeDelta { .. }
            | ServerMsg::StrokeCancel { .. }
            | ServerMsg::Resync { .. }
            | ServerMsg::Pong { .. } => {}
        }
        out.head_seq = self.replica.head_seq();
        Ok(out)
    }

    /// The next `PushOps` frame, when the session may send one: welcomed, not mid-snapshot, nothing in
    /// flight and the outbox is not empty. The `Ack` re-opens the gate.
    ///
    /// # Errors
    /// Frame encoding failure (never expected).
    pub fn poll_push(&mut self) -> Result<Option<Vec<u8>>, SyncError> {
        if !self.welcomed || self.snapshotting || self.inflight || self.outbox.is_empty() {
            return Ok(None);
        }
        let ops = self.outbox.next_batch();
        if ops.is_empty() {
            return Ok(None);
        }
        let frame = encode_client(&ClientMsg::PushOps { ops })?;
        self.inflight = true;
        Ok(Some(frame))
    }

    fn on_welcome(
        &mut self,
        server: penwise_proto::ServerId,
        server_now_ms: u64,
        now_ms: u64,
        out: &mut Outcome,
    ) {
        let reference = if self.hello_at == 0 {
            now_ms
        } else {
            self.hello_at
        };
        self.offset = penwise_proto::ClockOffset::from_welcome(server_now_ms, reference);
        self.welcomed = true;
        self.inflight = false;
        if self.pinned.is_some_and(|pinned| pinned != server) {
            let pages: Vec<_> = self.page_ids();
            self.rebuild_from_outbox();
            for page in pages {
                out.note(Change { page, item: None });
            }
            out.catalog_changed = true;
            out.reconnect = true;
        }
        self.pinned = Some(server);
    }

    /// A different server: drop everything it never issued, keep the unsent local ops.
    fn rebuild_from_outbox(&mut self) {
        self.replica = Replica::new();
        for op in self.outbox.pending_ops() {
            let _ = self.replica.apply(&op);
        }
    }

    fn on_snapshot_begin(&mut self, scope: Scope, out: &mut Outcome) {
        self.snapshotting = true;
        self.inflight = false;
        self.snapshot_pages.clear();
        match scope {
            Scope::All | Scope::Catalog => self.snapshot_pages.extend(self.page_ids()),
            Scope::Page(page) => {
                self.snapshot_pages.insert(page);
            }
        }
        if self.replica.begin_snapshot(scope).is_err() {
            out.apply_errors += 1;
        }
    }

    fn on_chunk(&mut self, chunk: &SnapshotChunk, now_ms: u64, out: &mut Outcome) {
        self.observe_chunk(chunk, now_ms);
        match self.replica.apply_snapshot_chunk(chunk) {
            Ok(applied) => {
                out.catalog_changed |= applied.catalog_changed;
                self.snapshot_pages.extend(applied.pages);
                for row in &chunk.items {
                    self.snapshot_pages.insert(row.page);
                }
            }
            Err(_) => out.apply_errors += 1,
        }
    }

    fn on_snapshot_end(&mut self, head_seq: u64, out: &mut Outcome) {
        self.replica.end_snapshot(head_seq);
        for op in self.outbox.pending_ops() {
            let _ = self.replica.apply(&op);
        }
        self.snapshotting = false;
        out.snapshot_done = true;
        out.catalog_changed = true;
        let pages = std::mem::take(&mut self.snapshot_pages);
        for page in pages {
            out.note(Change { page, item: None });
        }
    }

    /// Feeds every stamp in `chunk` to the clock so the next local stamp exceeds all of them.
    fn observe_chunk(&mut self, chunk: &SnapshotChunk, now_ms: u64) {
        let mut newest = Hlc::zero(self.device);
        let mut see = |hlc: Hlc| {
            if hlc > newest {
                newest = hlc;
            }
        };
        for state in &chunk.notebooks {
            [
                state.created,
                state.title_hlc,
                state.position_hlc,
                state.deleted_hlc,
            ]
            .into_iter()
            .for_each(&mut see);
        }
        for state in &chunk.pages {
            [
                state.created,
                state.title_hlc,
                state.position_hlc,
                state.background_hlc,
                state.deleted_hlc,
            ]
            .into_iter()
            .for_each(&mut see);
        }
        for row in &chunk.items {
            if let Ok(stamps) = FieldStamps::decode(&row.stamps) {
                see(stamps.created);
                for field in Field::ALL {
                    see(stamps.get(field));
                }
            }
        }
        let _ = self.hlc.recv(newest, self.local_now(now_ms));
    }

    /// Every page the replica knows.
    pub(super) fn page_ids(&self) -> Vec<penwise_proto::PageId> {
        self.replica
            .catalog()
            .page_states()
            .iter()
            .map(|p| p.id)
            .collect()
    }

    /// Item ids of a page in the replica, tombstones included (test and debug aid).
    #[must_use]
    pub fn item_ids(&self, page: penwise_proto::PageId) -> Vec<ItemId> {
        self.replica
            .page(page)
            .map(|p| p.items().map(|m| m.item.id).collect())
            .unwrap_or_default()
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
