//! Durable state: the op log and its materialized rows in `SQLite` (design §3).
//!
//! One **writer thread** owns the only write connection; every mutation is a command executed as a single
//! transaction (validate → log → materialize → commit), and the committed ops are broadcast only *after* the
//! commit. Reads use a small pool of read-only WAL connections (each read is a consistent snapshot). There is
//! no in-memory page cache that could diverge from the database.
//!
//! [`Storage`] is the cloneable async handle; sessions, HTTP routes and the harness all use it.

mod apply;
mod compact;
mod error;
mod generations;
mod legacy_codec;
pub mod migrate;
pub(crate) mod random;
mod read;
mod restore;
mod rounds;
mod rows;
mod schema;
mod snapshot;
mod strip_pages;
mod types;
mod writer;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use penwise_proto::{AnyOp, Catalog, DeviceId, Hlc, PageId, Scope, SeqOp, ServerId};
use tokio::sync::{broadcast, oneshot};

pub use compact::CompactReport;
pub use error::{AppendError, Rejected, RestoreError, StorageError};
pub use generations::{Generation, GenerationRole};
pub(crate) use rounds::RoundOp;
pub use rounds::{LedgerRow, Round, RoundStatus};
pub use schema::SCHEMA_VERSION;
pub use strip_pages::StripPage;
pub use types::{
    AliasTarget, Appended, DEVICE_IDLE_DAYS, DeviceInfo, FaultPlan, NowMs, OpFilter, OpsPage,
    PageView, RETENTION_DAYS, Restored, SNAPSHOT_CHUNK_ITEMS, SnapshotStream, Status,
    StorageConfig, system_now_ms,
};

pub(crate) struct Inner {
    pub(crate) commands: std::sync::mpsc::Sender<writer::Command>,
    pub(crate) readers: read::ReadPool,
    pub(crate) ops: broadcast::Sender<Arc<[SeqOp]>>,
    pub(crate) server_id: ServerId,
    pub(crate) server_device: DeviceId,
    pub(crate) now_ms: NowMs,
}

/// Cloneable handle to the writer thread and the read pool.
#[derive(Clone)]
pub struct Storage {
    pub(crate) inner: Arc<Inner>,
}

impl std::fmt::Debug for Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Storage")
            .field("server_id", &self.inner.server_id)
            .finish_non_exhaustive()
    }
}

impl Storage {
    /// Opens (creating) the database and starts the writer thread.
    ///
    /// # Errors
    /// `NeedsMigration` for a v1–3 file, `FutureSchema`, `Corrupt` (missing `server_meta`), `Sqlite`, `Io`.
    pub fn open(config: StorageConfig) -> Result<Self, StorageError> {
        writer::start(config)
    }

    /// This data directory's identity (pinned by clients).
    #[must_use]
    pub fn server_id(&self) -> ServerId {
        self.inner.server_id
    }

    /// The persistent device that authors server-side ops (annotate, clear, restore, picture place).
    #[must_use]
    pub fn server_device(&self) -> DeviceId {
        self.inner.server_device
    }

    /// The injected wall clock.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        (self.inner.now_ms)()
    }

    /// Subscribes to committed batches; send happens only after the commit, in `seq` order. Only the ops
    /// newly logged by a frame are sent (idempotent re-pushes send nothing).
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<[SeqOp]>> {
        self.inner.ops.subscribe()
    }

    /// Validates (design §2.4: device/skew/stateless/dedupe/references) and logs `ops` pushed by `device`
    /// in ONE transaction. All or nothing: any refused op rolls the frame back and is reported in
    /// `AppendError::Rejected` (ops depending on a refused op are refused too). Ops already logged (same
    /// `op_id`) return their original seq. Server-authored ops use `server_device()` as `device`.
    ///
    /// # Errors
    /// `Rejected` or `Storage` (`Fault`, `Sqlite`, `Closed`).
    pub async fn append_ops(
        &self,
        device: DeviceId,
        ops: Vec<AnyOp>,
    ) -> Result<Vec<Appended>, AppendError> {
        self.call(|reply| writer::Command::Append { device, ops, reply })
            .await?
    }

    /// A fresh stamp from the server device's clock, greater than every stamp the server has logged.
    /// Build server-authored ops with it (it need not be used immediately).
    ///
    /// # Errors
    /// `Closed`.
    pub async fn server_stamp(&self) -> Result<Hlc, StorageError> {
        self.call(|reply| writer::Command::ServerStamp { reply })
            .await
    }

    /// Registers or refreshes a device (`last_seen_ms = now`) and raises its acked seq (`Hello.last_seen_seq`,
    /// or the delivery cursor at disconnect); the ack never moves backwards.
    ///
    /// # Errors
    /// `Closed`, `Sqlite`.
    pub async fn touch_device(
        &self,
        device: DeviceInfo,
        acked_seq: u64,
    ) -> Result<(), StorageError> {
        self.call(|reply| writer::Command::Touch {
            device,
            acked_seq,
            reply,
        })
        .await?
    }

    /// Head seq and log floor.
    ///
    /// # Errors
    /// `Sqlite`.
    pub async fn status(&self) -> Result<Status, StorageError> {
        self.read(read::status).await
    }

    /// One batch of ops after `after_seq` (at most `limit`, ≥ 1) in `filter`'s scope.
    ///
    /// # Errors
    /// `BelowFloor` when `after_seq < log_floor` (send a snapshot instead), `Corrupt`, `Sqlite`.
    pub async fn ops_since(
        &self,
        after_seq: u64,
        filter: OpFilter,
        limit: usize,
    ) -> Result<OpsPage, StorageError> {
        self.read(move |conn| read::ops_since(conn, after_seq, filter, limit))
            .await
    }

    /// Streams a snapshot of `scope` (notebooks, pages, then items; ≤ [`SNAPSHOT_CHUNK_ITEMS`] items per
    /// chunk) read in one read transaction at `head_seq`. Dropping the stream stops the read.
    ///
    /// # Errors
    /// `Sqlite` when the transaction cannot start; later failures arrive through the channel.
    pub async fn snapshot(&self, scope: Scope) -> Result<SnapshotStream, StorageError> {
        snapshot::start(self, scope).await
    }

    /// The materialized page, `None` when the page does not exist.
    ///
    /// # Errors
    /// `Corrupt`, `Sqlite`.
    pub async fn page_view(&self, page: PageId) -> Result<Option<PageView>, StorageError> {
        self.read(move |conn| read::page_view(conn, page)).await
    }

    /// Every notebook and page with registers (tombstones included).
    ///
    /// # Errors
    /// `Corrupt`, `Sqlite`.
    pub async fn catalog(&self) -> Result<Catalog, StorageError> {
        self.read(read::catalog).await
    }

    /// Live (non-deleted) item ids of `page` on `layer` code (agent marks lookup for clear).
    ///
    /// # Errors
    /// `Sqlite`.
    pub async fn live_item_ids(
        &self,
        page: PageId,
        layer: Option<penwise_core::item::Layer>,
    ) -> Result<Vec<penwise_core::item::ItemId>, StorageError> {
        self.read(move |conn| read::live_item_ids(conn, page, layer))
            .await
    }

    /// Resolves a pre-rewrite `pg_…` / `nb_…` id (migration alias table).
    ///
    /// # Errors
    /// `Sqlite`.
    pub async fn resolve_alias(&self, legacy: String) -> Result<Option<AliasTarget>, StorageError> {
        self.read(move |conn| read::resolve_alias(conn, &legacy))
            .await
    }

    /// The page holding `item`, for routes that address an item without a page (`POST /marks/<id>/status`).
    /// Deleted items and pages are included; the handler decides what a missing page means.
    ///
    /// # Errors
    /// `Sqlite`.
    pub async fn page_of_item(
        &self,
        item: penwise_core::item::ItemId,
    ) -> Result<Option<PageId>, StorageError> {
        self.read(move |conn| read::page_of_item(conn, item)).await
    }

    /// The server's "current page" (`server_meta.current_page`).
    ///
    /// # Errors
    /// `Sqlite`.
    pub async fn current_page(&self) -> Result<Option<PageId>, StorageError> {
        self.read(read::current_page).await
    }

    /// Sets the current page.
    ///
    /// # Errors
    /// `Closed`, `Sqlite`.
    pub async fn set_current_page(&self, page: PageId) -> Result<(), StorageError> {
        self.call(|reply| writer::Command::SetCurrentPage { page, reply })
            .await?
    }

    /// The op-log seq `agent` last read marks up to (design-harness C2); `None` before its first call.
    ///
    /// # Errors
    /// `Sqlite`.
    pub async fn agent_checkpoint(&self, agent: String) -> Result<Option<u64>, StorageError> {
        self.read(move |conn| read::agent_checkpoint(conn, &agent))
            .await
    }

    /// Records `agent`'s read checkpoint; every `GET /marks` call advances it.
    ///
    /// # Errors
    /// `Closed`, `Sqlite`.
    pub async fn set_agent_checkpoint(&self, agent: String, seq: u64) -> Result<(), StorageError> {
        let at_ms = self.now_ms();
        self.call(|reply| writer::Command::SetAgentCheckpoint {
            agent,
            seq,
            at_ms,
            reply,
        })
        .await?
    }

    /// `(id, updated_seq)` of every item row of `page`; the seq tells the agent what changed since a checkpoint.
    ///
    /// # Errors
    /// `Sqlite`.
    pub async fn item_seqs(
        &self,
        page: PageId,
    ) -> Result<Vec<(penwise_core::item::ItemId, u64)>, StorageError> {
        self.read(move |conn| read::item_seqs(conn, page)).await
    }

    /// Every round of `project`, oldest first (design-harness spec v2 §4).
    ///
    /// # Errors
    /// `Sqlite`, `Corrupt`.
    pub async fn rounds(&self, project: String) -> Result<Vec<Round>, StorageError> {
        self.read(move |conn| rounds::list(conn, &project)).await
    }

    /// Runs a change on the `rounds` table; `None` when the named round does not exist.
    ///
    /// # Errors
    /// `Closed`, `Sqlite`.
    pub(crate) async fn round_op(&self, op: RoundOp) -> Result<Option<Round>, StorageError> {
        let at_ms = self.now_ms();
        self.call(|reply| writer::Command::Round { op, at_ms, reply })
            .await?
    }

    /// Records the pictures a resolve froze (replacing rows of the same lane, role and label).
    ///
    /// # Errors
    /// `Closed`, `Sqlite`.
    pub async fn record_generations(&self, rows: Vec<Generation>) -> Result<(), StorageError> {
        self.call(|reply| writer::Command::Generations { rows, reply })
            .await?
    }

    /// Attaches `page` to the strip of `route` and `preset` in `project` (idempotent; attaching again moves it).
    ///
    /// # Errors
    /// `Closed`, `Sqlite`.
    pub async fn attach_strip_page(
        &self,
        project: String,
        route: String,
        preset: String,
        page: PageId,
    ) -> Result<(), StorageError> {
        let row = StripPage {
            page,
            route,
            preset,
        };
        self.call(|reply| writer::Command::StripPage {
            project,
            row,
            reply,
        })
        .await?
    }

    /// The pages attached to strips of `project`, in attach order; deleted pages are left out.
    ///
    /// # Errors
    /// `Sqlite`.
    pub async fn strip_pages(&self, project: String) -> Result<Vec<StripPage>, StorageError> {
        self.read(move |conn| strip_pages::list(conn, &project))
            .await
    }

    /// The kept pictures of resolved rounds of `project`, newest round first, optionally of one lane.
    ///
    /// # Errors
    /// `Sqlite`, `Corrupt`.
    pub async fn generations(
        &self,
        project: String,
        lane: Option<(String, String)>,
    ) -> Result<Vec<Generation>, StorageError> {
        self.read(move |conn| generations::list(conn, &project, lane.as_ref()))
            .await
    }

    /// The live frame and mark items of `page` with their add and update seqs (the round ledger's view of a page).
    ///
    /// # Errors
    /// `Sqlite`, `Corrupt`.
    pub async fn frames_and_marks(&self, page: PageId) -> Result<Vec<LedgerRow>, StorageError> {
        self.read(move |conn| rounds::frames_and_marks(conn, page))
            .await
    }

    /// `(id, added_seq)` of every item row of `page`: the seq of the op that added each item.
    ///
    /// # Errors
    /// `Sqlite`.
    pub async fn item_added_seqs(
        &self,
        page: PageId,
    ) -> Result<Vec<(penwise_core::item::ItemId, u64)>, StorageError> {
        self.read(move |conn| rounds::added_seqs(conn, page)).await
    }

    /// Restores `page` to its state at `to_seq` by appending compensating ops from the server device in one
    /// batch (design §3.5). Returns what was appended (empty when nothing differs) and the `skipped` items
    /// whose history before the log floor is gone and that changed since `to_seq`.
    ///
    /// # Errors
    /// `UnknownPage` (404), `BelowFloor` (410), `Append`, `Storage`.
    pub async fn restore_page(&self, page: PageId, to_seq: u64) -> Result<Restored, RestoreError> {
        self.call(|reply| writer::Command::Restore {
            page,
            to_seq,
            reply,
        })
        .await?
    }

    /// Runs retention now, in slices of at most 5,000 rows so the writer is never held long: drops log rows and
    /// tombstoned items older than [`RETENTION_DAYS`] that every
    /// active device has acked, and raises `log_floor`.
    ///
    /// # Errors
    /// `Closed`, `Sqlite`.
    pub async fn compact(&self) -> Result<CompactReport, StorageError> {
        let mut total = CompactReport {
            log_floor: 0,
            ops_deleted: 0,
            items_purged: 0,
        };
        loop {
            let step = self
                .call(|reply| writer::Command::Compact { reply })
                .await??;
            total.log_floor = step.report.log_floor;
            total.ops_deleted += step.report.ops_deleted;
            total.items_purged += step.report.items_purged;
            if !step.more {
                return Ok(total);
            }
            // Each slice is its own writer command, so pushes and acks queued meanwhile run between slices.
            tokio::task::yield_now().await;
        }
    }

    async fn call<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<T>) -> writer::Command,
    ) -> Result<T, StorageError> {
        let (reply, rx) = oneshot::channel();
        self.inner
            .commands
            .send(make(reply))
            .map_err(|_| StorageError::Closed)?;
        rx.await.map_err(|_| StorageError::Closed)
    }

    pub(crate) async fn read<T: Send + 'static>(
        &self,
        f: impl FnOnce(&rusqlite::Connection) -> Result<T, StorageError> + Send + 'static,
    ) -> Result<T, StorageError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.readers.with(f))
            .await
            .map_err(|_| StorageError::Closed)?
    }
}
