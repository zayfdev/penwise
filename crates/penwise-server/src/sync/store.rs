//! What a sync session needs from storage, as one trait.
//!
//! `storage::Storage` implements it (`storage_adapter.rs`); unit tests implement it with an in-memory fake
//! (`tests/fake.rs`). Every method is a plain async call: the single writer thread and the read pool live
//! behind it, no lock is held across an await.

use std::future::Future;
use std::sync::Arc;

use penwise_proto::{
    AnyOp, ClientKind, DeviceId, OpId, OpRejection, PageId, Scope, SeqOp, SnapshotChunk,
};
use tokio::sync::{broadcast, mpsc};

/// Storage failed; the session ends with `Error{Internal}` and the client retries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("storage: {0}")]
pub struct StoreError(pub String);

/// Log position facts read at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogStatus {
    pub head_seq: u64,
    /// Highest purged seq: a client whose `last_seen_seq` is below it needs a snapshot.
    pub log_floor: u64,
}

/// Which logged ops a session receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpFilter {
    /// Every op (tablet, agent).
    All,
    /// Notebook and page ops, plus item ops of `page` with `seq > page_from_seq` (browser). Storage may
    /// return item ops at or below `page_from_seq` (it only knows `page`); sessions drop them.
    Catalog {
        page: Option<PageId>,
        page_from_seq: u64,
    },
}

/// One page of `ops_since`: every op with `after < seq ≤ upto_seq` that passes the filter. `upto_seq` is the
/// last seq examined (`≥ after`; `head_seq` when the log is exhausted), so a filtered page can skip a gap.
#[derive(Debug, Clone, PartialEq)]
pub struct OpsPage {
    pub ops: Vec<SeqOp>,
    pub upto_seq: u64,
    pub head_seq: u64,
}

/// Answer of `ops_since`.
#[derive(Debug, Clone, PartialEq)]
pub enum OpsRead {
    Page(OpsPage),
    /// `after` is below the retained log: send a snapshot.
    BelowFloor,
}

/// Why `append_ops` logged nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppendFailure {
    /// The frame was rolled back because these ops are refused (every refused op is listed).
    Rejected(Vec<(OpId, OpRejection)>),
    Store(StoreError),
}

/// A snapshot read in one consistent transaction: `head_seq` first, then chunks (≤ 512 items each) as the
/// reader produces them; an `Err` item means the read failed.
#[derive(Debug)]
pub struct SnapshotRead {
    pub head_seq: u64,
    pub chunks: mpsc::Receiver<Result<SnapshotChunk, StoreError>>,
}

/// Storage as the sync layer sees it. Futures are `Send` so sessions can be spawned.
pub trait SyncStore: Clone + Send + Sync + 'static {
    /// Head and floor.
    fn status(&self) -> impl Future<Output = Result<LogStatus, StoreError>> + Send;

    /// The server's identity (pinned by clients).
    fn server_id(&self) -> penwise_proto::ServerId;

    /// Validate → log → materialize → commit one frame atomically; the seq of each op in order (a known op id
    /// reports its original seq). All or nothing: a refused op rolls the frame back
    /// ([`AppendFailure::Rejected`]). Committed batches are broadcast *after* commit.
    fn append_ops(
        &self,
        device: DeviceId,
        ops: Vec<AnyOp>,
    ) -> impl Future<Output = Result<Vec<u64>, AppendFailure>> + Send;

    /// Ops after `after` that pass `filter`, at most `limit`.
    fn ops_since(
        &self,
        after: u64,
        filter: OpFilter,
        limit: usize,
    ) -> impl Future<Output = Result<OpsRead, StoreError>> + Send;

    /// Row dump of `scope`, consistent with its `head_seq`.
    fn snapshot(
        &self,
        scope: Scope,
    ) -> impl Future<Output = Result<SnapshotRead, StoreError>> + Send;

    /// Committed batches, contiguous and in seq order, published only after commit. Subscribe *before*
    /// reading a snapshot or log page and skip what the cursor already covers.
    fn subscribe(&self) -> broadcast::Receiver<Arc<[SeqOp]>>;

    /// Records that `device` (of `kind`) has everything up to `acked_seq` (holds the retention floor).
    fn note_device(
        &self,
        device: DeviceId,
        kind: ClientKind,
        acked_seq: u64,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// The last page any session switched to.
    fn current_page(&self) -> impl Future<Output = Result<Option<PageId>, StoreError>> + Send;

    fn set_current_page(&self, page: PageId)
    -> impl Future<Output = Result<(), StoreError>> + Send;
}
