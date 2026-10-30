//! The I/O half of `TestDevice`: flushing the outbox, raw sends and handling server frames.

use std::time::Duration;

use penwise_proto::{ClientMsg, OpId, ServerMsg};

use crate::TestDevice;
use crate::socket::Recv;

impl TestDevice {
    /// Sends the next outbox batch when nothing is in flight.
    pub async fn flush(&mut self) {
        if !self.inflight.is_empty() || self.outbox.is_empty() || self.snapshotting {
            return;
        }
        let batch = self.outbox.next_batch();
        if batch.is_empty() {
            return;
        }
        let ids: Vec<OpId> = batch.iter().map(penwise_proto::AnyOp::op_id).collect();
        if self.send(&ClientMsg::PushOps { ops: batch }).await.is_ok() {
            self.inflight.extend(ids);
        }
    }

    /// Sends one client message.
    ///
    /// # Errors
    /// Offline or transport error text.
    pub async fn send(&mut self, msg: &ClientMsg) -> Result<(), String> {
        match self.conn.as_mut() {
            Some(conn) => conn.send(msg).await,
            None => Err("offline".to_owned()),
        }
    }

    /// Sends raw bytes as one binary frame.
    ///
    /// # Errors
    /// Offline or transport error text.
    pub async fn send_raw(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        match self.conn.as_mut() {
            Some(conn) => conn.send_bytes(bytes).await,
            None => Err("offline".to_owned()),
        }
    }

    /// Sends a text frame (a protocol violation).
    ///
    /// # Errors
    /// Offline or transport error text.
    pub async fn send_text(&mut self, text: &str) -> Result<(), String> {
        match self.conn.as_mut() {
            Some(conn) => conn.send_text(text).await,
            None => Err("offline".to_owned()),
        }
    }

    /// Reads and handles one frame; `false` on timeout or close.
    pub(crate) async fn pump_one(&mut self, timeout: Duration) -> bool {
        let Some(conn) = self.conn.as_mut() else {
            return false;
        };
        match conn.recv(timeout).await {
            Recv::Msg(msg) => {
                if let ServerMsg::Ping { nonce } = msg {
                    let _ = self.send(&ClientMsg::Pong { nonce }).await;
                }
                self.handle(msg);
                true
            }
            Recv::Closed(code) => {
                self.events.closed = Some(code);
                self.offline();
                false
            }
            Recv::Timeout => false,
            Recv::Undecodable(_) | Recv::Text(_) => true,
        }
    }

    fn handle(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::Ack {
                op_ids,
                seqs,
                rejected,
            } => {
                self.outbox.ack(&op_ids);
                self.outbox.reject(&rejected);
                // One Ack answers the whole frame (all-or-nothing): whatever was not acked or refused is
                // pending again and goes out in the next frame.
                self.inflight.clear();
                self.events.acks.push((op_ids, seqs));
                self.events.rejected.extend(rejected);
            }
            ServerMsg::OpsSince { upto_seq, ops, .. } => {
                for op in &ops {
                    self.events.delivered.push(op.seq);
                    let _ = self.hlc.observe_logged(op.op.hlc(), self.local_now());
                    if let Err(e) = self.replica.apply_seq(op) {
                        self.events.apply_errors.push(e.to_string());
                    }
                }
                self.replica.advance_to(upto_seq);
            }
            ServerMsg::SnapshotBegin { scope, .. } => {
                self.snapshotting = true;
                self.snapshot_scope = scope;
                self.inflight.clear();
                if let Err(e) = self.replica.begin_snapshot(scope) {
                    self.events.apply_errors.push(e.to_string());
                }
            }
            ServerMsg::SnapshotChunk(chunk) => {
                let now = self.local_now();
                let _ = penwise_proto::observe_snapshot_chunk(&mut self.hlc, &chunk, now);
                if let Err(e) = self.replica.apply_snapshot_chunk(&chunk) {
                    self.events.apply_errors.push(e.to_string());
                }
            }
            ServerMsg::SnapshotEnd { head_seq } => {
                self.replica.end_snapshot(self.snapshot_scope, head_seq);
                for op in self.outbox.pending_ops() {
                    let _ = self.replica.apply(&op);
                }
                self.snapshotting = false;
                self.events.snapshots += 1;
            }
            ServerMsg::StrokeDelta { device, delta } => self.events.deltas.push((device, delta)),
            ServerMsg::StrokeCancel { device, cancel } => {
                self.events.cancels.push((device, cancel));
            }
            ServerMsg::Resync { from_seq } => self.events.resyncs.push(from_seq),
            ServerMsg::Error { code, detail } => self.events.errors.push((code, detail)),
            ServerMsg::Reject { reason } => self.events.rejects.push(reason),
            ServerMsg::Welcome { .. }
            | ServerMsg::SwitchPage { .. }
            | ServerMsg::Focus { .. }
            | ServerMsg::Ping { .. }
            | ServerMsg::Pong { .. } => {}
        }
    }
}
