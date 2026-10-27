//! Getting committed ops and snapshots to one client: live batches, gap reads, catch-up, snapshots, `Watch`.

use penwise_proto::message::MAX_OPS_PER_FRAME;
use penwise_proto::{
    AnyOp, ClientKind, ItemRef, PageId, ProtoError, Scope, SeqOp, ServerMsg, encode_server,
};

use super::cursor::{Delivery, classify, visible};
use super::outbound::Stop;
use super::session::Session;
use super::store::{OpsRead, SyncStore};

/// One `OpsSince` frame still to send (it may be split further).
struct Chunk {
    from: u64,
    upto: u64,
    ops: Vec<SeqOp>,
    more: bool,
}

impl Chunk {
    /// Splits after `at` ops: the first part ends at that op's seq, the second continues from there.
    fn split(mut self, at: usize) -> (Chunk, Chunk) {
        let tail = self.ops.split_off(at);
        let mid = self.ops.last().map_or(self.from, |op| op.seq);
        let second = Chunk {
            from: mid,
            upto: self.upto,
            ops: tail,
            more: self.more,
        };
        let first = Chunk {
            from: self.from,
            upto: mid,
            ops: self.ops,
            more: true,
        };
        (first, second)
    }
}

impl<S: SyncStore> Session<S> {
    /// A committed batch from the broadcast: deliver the new part, read a gap, or skip.
    pub(super) async fn on_batch(&mut self, batch: &[SeqOp]) -> Result<(), Stop> {
        match classify(self.cursor, batch) {
            Delivery::Skip => Ok(()),
            Delivery::Deliver { start } => self.deliver(&batch[start..]).await,
            Delivery::Gap => self.catch_up().await,
        }
    }

    /// Sends the visible part of a contiguous run starting at `cursor + 1` and advances the cursor over all of
    /// it (a filtered-out run still moves the cursor).
    async fn deliver(&mut self, run: &[SeqOp]) -> Result<(), Stop> {
        let Some(last) = run.last() else {
            return Ok(());
        };
        let upto = last.seq;
        self.forget_committed_strokes(run);
        let filter = self.filter();
        let ops: Vec<SeqOp> = run
            .iter()
            .filter(|op| visible(filter, op))
            .cloned()
            .collect();
        if !ops.is_empty() {
            self.send_ops(self.cursor, upto, ops, upto, false).await?;
        }
        self.cursor = upto;
        Ok(())
    }

    /// Own `Add`s echoing back close the matching live stroke (the committed item replaces the preview).
    fn forget_committed_strokes(&mut self, run: &[SeqOp]) {
        for op in run {
            if let AnyOp::Item(item) = &op.op
                && item.device == self.device
                && matches!(item.edit, penwise_core::edit::Edit::Add(_))
            {
                self.open.remove(&ItemRef::from(item.edit.target()));
            }
        }
    }

    /// Reads and sends every op after the cursor, in `OpsSince` pages, until the head. Always sends at least
    /// one frame (an empty `more == false` batch tells the client it is caught up).
    pub(super) async fn catch_up(&mut self) -> Result<(), Stop> {
        let mut sent = false;
        loop {
            let read = self
                .hub
                .store
                .ops_since(self.cursor, self.filter(), MAX_OPS_PER_FRAME)
                .await
                .map_err(|e| Stop::internal(&e))?;
            let OpsRead::Page(mut page) = read else {
                // The log moved past the cursor (compaction): only a snapshot is complete.
                return self.send_snapshots().await;
            };
            let filter = self.filter();
            page.ops.retain(|op| visible(filter, op));
            let progressed = page.upto_seq > self.cursor;
            let more = progressed && page.upto_seq < page.head_seq;
            if progressed || !sent {
                let upto = page.upto_seq.max(self.cursor);
                self.send_ops(self.cursor, upto, page.ops, page.head_seq, more)
                    .await?;
                sent = true;
                self.cursor = upto;
            }
            if !more {
                return Ok(());
            }
        }
    }

    /// Splits as needed (op count, frame size) and sends `ops` covering `from < seq ≤ upto`.
    async fn send_ops(
        &self,
        from: u64,
        upto: u64,
        ops: Vec<SeqOp>,
        head: u64,
        more: bool,
    ) -> Result<(), Stop> {
        let mut pending = vec![Chunk {
            from,
            upto,
            ops,
            more,
        }];
        while let Some(chunk) = pending.pop() {
            if chunk.ops.len() > MAX_OPS_PER_FRAME {
                let (first, second) = chunk.split(MAX_OPS_PER_FRAME);
                pending.push(second);
                pending.push(first);
                continue;
            }
            let msg = ServerMsg::OpsSince {
                from_seq: chunk.from,
                upto_seq: chunk.upto,
                ops: chunk.ops,
                head_seq: head,
                more: chunk.more,
            };
            match encode_server(&msg) {
                Ok(frame) => self.out.send_frame(frame).await?,
                Err(ProtoError::TooLarge { .. }) => {
                    let ServerMsg::OpsSince {
                        from_seq,
                        upto_seq,
                        ops,
                        more,
                        ..
                    } = msg
                    else {
                        return Err(Stop::internal(&"unexpected message"));
                    };
                    if ops.len() < 2 {
                        return Err(Stop::internal(&"one op exceeds the frame limit"));
                    }
                    let half = ops.len() / 2;
                    let (first, second) = Chunk {
                        from: from_seq,
                        upto: upto_seq,
                        ops,
                        more,
                    }
                    .split(half);
                    pending.push(second);
                    pending.push(first);
                }
                Err(error) => return Err(Stop::internal(&error)),
            }
        }
        Ok(())
    }

    /// Continues from `from`: a snapshot when the client is behind the floor, ahead of the head, or `force`d,
    /// otherwise the log after `from` (the cursor moves to `from`; deliveries at or below it are skipped).
    pub(super) async fn resync_from(
        &mut self,
        from: u64,
        force_snapshot: bool,
    ) -> Result<(), Stop> {
        let status = self
            .hub
            .store
            .status()
            .await
            .map_err(|e| Stop::internal(&e))?;
        if force_snapshot || from < status.log_floor || from > status.head_seq {
            return self.send_snapshots().await;
        }
        self.cursor = from;
        self.catch_up().await
    }

    /// Full snapshot for this kind of client; the cursor lands on its `head_seq`.
    async fn send_snapshots(&mut self) -> Result<(), Stop> {
        match self.kind {
            ClientKind::Tablet | ClientKind::Agent => {
                self.cursor = self.send_snapshot(Scope::All).await?;
            }
            ClientKind::Browser => {
                self.cursor = self.send_snapshot(Scope::Catalog).await?;
                if let Some(page) = self.watch {
                    self.page_from_seq = self.send_snapshot(Scope::Page(page)).await?;
                }
            }
        }
        Ok(())
    }

    /// `SnapshotBegin` · chunks · `SnapshotEnd`; returns the snapshot's `head_seq`.
    async fn send_snapshot(&self, scope: Scope) -> Result<u64, Stop> {
        let mut read = self
            .hub
            .store
            .snapshot(scope)
            .await
            .map_err(|e| Stop::internal(&e))?;
        let head_seq = read.head_seq;
        self.out
            .send(&ServerMsg::SnapshotBegin { head_seq, scope })
            .await?;
        while let Some(chunk) = read.chunks.recv().await {
            let chunk = chunk.map_err(|e| Stop::internal(&e))?;
            self.out.send(&ServerMsg::SnapshotChunk(chunk)).await?;
        }
        self.out.send(&ServerMsg::SnapshotEnd { head_seq }).await?;
        Ok(head_seq)
    }

    /// `Watch`: picks the page whose live deltas arrive; a browser also gets that page's snapshot first and
    /// item ops only after it.
    pub(super) async fn on_watch(&mut self, page: Option<PageId>) -> Result<(), Stop> {
        if self.kind == ClientKind::Browser {
            if let Some(page) = page {
                // Deliver every catalog op committed so far first: the page snapshot is taken at the head,
                // and a client must never see a head past catalog ops it was not sent. A `Page` snapshot does
                // not move the client's head (only `page_from_seq` here); ops committed during the snapshot
                // follow through the broadcast.
                let head = self
                    .hub
                    .store
                    .status()
                    .await
                    .map_err(|e| Stop::internal(&e))?
                    .head_seq;
                if head > self.cursor {
                    self.catch_up().await?;
                }
                // Filter switches only after the snapshot: ops of `page` at or below its head are inside it.
                let head = self.send_snapshot(Scope::Page(page)).await?;
                self.page_from_seq = head;
            } else {
                self.page_from_seq = 0;
            }
        }
        self.watch = page;
        Ok(())
    }
}
