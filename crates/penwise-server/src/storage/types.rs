//! Plain types of the storage API: configuration, fault injection, results.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use penwise_proto::{
    ClientKind, DeviceId, NotebookId, OpId, Page, PageId, PageReplica, SeqOp, SnapshotChunk,
};
use tokio::sync::mpsc;

use super::error::StorageError;

/// Wall clock in Unix ms (injectable for tests).
pub type NowMs = Arc<dyn Fn() -> u64 + Send + Sync>;

/// Retention of log and tombstones (owner decision 2026-10-27).
pub const RETENTION_DAYS: u64 = 7;
/// A device unseen this long stops holding the log floor.
pub const DEVICE_IDLE_DAYS: u64 = 30;
/// Ops per snapshot chunk.
pub const SNAPSHOT_CHUNK_ITEMS: usize = 512;

/// Fault injection: the first frame that inserts at least `after_ops` ops fails (rolled back,
/// `StorageError::Fault`), then the plan disarms.
#[derive(Debug, Clone)]
pub struct FaultPlan {
    pub after_ops: usize,
    armed: Arc<AtomicBool>,
}

impl FaultPlan {
    #[must_use]
    pub fn new(after_ops: usize) -> Self {
        Self {
            after_ops,
            armed: Arc::new(AtomicBool::new(true)),
        }
    }

    pub(crate) fn fire(&self, inserted: usize) -> bool {
        inserted >= self.after_ops && self.armed.swap(false, Ordering::SeqCst)
    }
}

/// How to open [`Storage`].
#[allow(
    missing_debug_implementations,
    reason = "holds a closure; nothing logs the config"
)]
#[derive(Clone)]
pub struct StorageConfig {
    /// Database file (created with schema v4 when missing; a v1–3 file is refused, see [`migrate`]).
    pub path: PathBuf,
    pub now_ms: NowMs,
    /// Capacity of the committed-ops broadcast (batches).
    pub broadcast_capacity: usize,
    pub fault: Option<FaultPlan>,
}

impl StorageConfig {
    /// System clock, capacity 1024, no fault plan.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            now_ms: Arc::new(system_now_ms),
            broadcast_capacity: 1024,
            fault: None,
        }
    }
}

/// System wall clock, Unix ms.
#[must_use]
pub fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Outcome of one op of an accepted frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Appended {
    pub op_id: OpId,
    pub seq: u64,
    /// The op was already logged (idempotent re-push): `seq` is the original, nothing was broadcast.
    pub duplicate: bool,
}

/// Which ops a session wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpFilter {
    /// Every op (tablet, agent).
    All,
    /// Notebook and page ops, plus item ops of the watched page (browser).
    CatalogAnd(Option<PageId>),
}

/// One `ops_since` batch: every op with `after_seq < seq ≤ upto_seq` in the filter's scope.
#[derive(Debug, Clone, PartialEq)]
pub struct OpsPage {
    pub ops: Vec<SeqOp>,
    pub upto_seq: u64,
    pub head_seq: u64,
    /// More ops follow after `upto_seq` (`upto_seq < head_seq`).
    pub more: bool,
}

/// Log position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub head_seq: u64,
    /// Ops with `seq ≤ log_floor` were compacted away; a client that has seen `< log_floor` needs a snapshot.
    pub log_floor: u64,
}

/// A snapshot in flight: `head_seq` is the position every chunk is consistent with (one read transaction).
#[allow(missing_debug_implementations, reason = "holds a channel receiver")]
pub struct SnapshotStream {
    pub head_seq: u64,
    pub chunks: mpsc::Receiver<Result<SnapshotChunk, StorageError>>,
}

/// A page ready to render: catalog row plus the replica built from its stored items.
#[derive(Debug, Clone)]
pub struct PageView {
    pub page: Page,
    pub replica: PageReplica,
    /// Highest `updated_seq` of the page row and its items: a cache key that changes on every write.
    pub updated_seq: u64,
}

/// Registration data of a connecting device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: DeviceId,
    pub name: String,
    pub kind: ClientKind,
}

/// What a legacy id resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasTarget {
    Notebook(NotebookId),
    Page(PageId),
}
