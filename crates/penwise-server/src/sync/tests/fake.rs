//! In-memory [`SyncStore`] with the behaviours sessions depend on: all-or-nothing appends, post-commit
//! broadcast, floor, filtered log pages, snapshot streams, and hooks to provoke races.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use penwise_core::Color;
use penwise_core::item::{Affine, Item, ItemId, ItemKind, Layer, Paint, TextItem};
use penwise_proto::{
    AnyOp, ClientKind, DeviceId, Hlc, NotebookChange, NotebookId, NotebookOp, Op, OpId,
    OpRejection, PageChange, PageId, PageKind, PageOp, Scope, SeqOp, SnapshotChunk,
};
use tokio::sync::{broadcast, mpsc};

use crate::sync::{
    AppendFailure, LogStatus, OpFilter, OpsPage, OpsRead, SnapshotRead, StoreError, SyncStore,
};

pub fn device(n: u64) -> DeviceId {
    DeviceId::from_parts(1_700_000_000_000, u128::from(n))
}
pub fn op_id(n: u64) -> OpId {
    OpId::from_parts(1_700_000_000_000, u128::from(n))
}
pub fn page_id(n: u64) -> PageId {
    PageId::from_parts(1_700_000_000_000, u128::from(n))
}
pub fn notebook_id(n: u64) -> NotebookId {
    NotebookId::from_parts(1_700_000_000_000, u128::from(n))
}

pub fn hlc(dev: u64, wall_ms: u64) -> Hlc {
    Hlc {
        wall_ms,
        counter: 0,
        device: device(dev),
    }
}

/// Notebook-create op number `n` by device `dev`.
pub fn notebook_op(dev: u64, n: u64) -> AnyOp {
    AnyOp::Notebook(NotebookOp {
        op_id: op_id(n),
        device: device(dev),
        hlc: hlc(dev, 1_700_000_000_000 + n),
        notebook: notebook_id(n),
        change: NotebookChange::Create {
            title: format!("nb {n}"),
            position: 0,
        },
    })
}

/// Page-create op number `n`.
pub fn page_op(dev: u64, n: u64, page: PageId) -> AnyOp {
    AnyOp::Page(PageOp {
        op_id: op_id(n),
        device: device(dev),
        hlc: hlc(dev, 1_700_000_000_000 + n),
        page,
        change: PageChange::Create {
            notebook: notebook_id(1),
            title: "p".to_owned(),
            position: 0,
            kind: PageKind::Infinite,
            background: penwise_proto::Background::Blank,
        },
    })
}

pub fn text_item(n: u64, content: &str) -> Item {
    Item {
        id: ItemId(penwise_proto::ItemRef::from_parts(1_700_000_000_000, u128::from(n)).0),
        layer: Layer::Ink,
        z: 0,
        transform: Affine::IDENTITY,
        kind: ItemKind::Text(TextItem {
            content: content.to_owned(),
            size: [120.0, 40.0],
            font_size: 24.0,
            paint: Paint {
                color: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                opacity: 1.0,
            },
            sources: Vec::new(),
        }),
    }
}

/// Item `Add` op number `n` on `page`.
pub fn item_op(dev: u64, n: u64, page: PageId, content: &str) -> AnyOp {
    AnyOp::Item(Box::new(Op {
        op_id: op_id(n),
        device: device(dev),
        hlc: hlc(dev, 1_700_000_000_000 + n),
        page,
        edit: penwise_core::edit::Edit::Add(text_item(n, content)),
    }))
}

#[derive(Default)]
struct State {
    log: Vec<SeqOp>,
    floor: u64,
    known: HashSet<OpId>,
    current_page: Option<PageId>,
    noted: Vec<(DeviceId, ClientKind, u64)>,
    snapshot_calls: Vec<Scope>,
    fail_status: bool,
    below_floor_once: bool,
}

type Hook = Box<dyn Fn(&FakeStore) + Send + Sync>;

#[derive(Clone)]
pub struct FakeStore {
    state: Arc<Mutex<State>>,
    tx: broadcast::Sender<Arc<[SeqOp]>>,
    on_snapshot: Arc<Mutex<Option<Hook>>>,
}

impl FakeStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            state: Arc::default(),
            tx: broadcast::channel(capacity).0,
            on_snapshot: Arc::default(),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Logs `ops` without broadcasting; returns the new batch.
    pub fn commit_silently(&self, ops: Vec<AnyOp>) -> Arc<[SeqOp]> {
        let mut st = self.state();
        let mut batch = Vec::new();
        for op in ops {
            if !st.known.insert(op.op_id()) {
                continue;
            }
            let seq = st.log.len() as u64 + 1;
            let entry = SeqOp { seq, op };
            st.log.push(entry.clone());
            batch.push(entry);
        }
        batch.into()
    }

    pub fn publish(&self, batch: Arc<[SeqOp]>) {
        let _ = self.tx.send(batch);
    }

    /// Logs and broadcasts (what a committed `PushOps` does).
    pub fn commit(&self, ops: Vec<AnyOp>) -> Arc<[SeqOp]> {
        let batch = self.commit_silently(ops);
        if !batch.is_empty() {
            self.publish(Arc::clone(&batch));
        }
        batch
    }

    pub fn set_floor(&self, floor: u64) {
        self.state().floor = floor;
    }

    pub fn fail_next_ops_since_below_floor(&self) {
        self.state().below_floor_once = true;
    }

    pub fn set_fail_status(&self, fail: bool) {
        self.state().fail_status = fail;
    }

    pub fn on_snapshot(&self, hook: impl Fn(&FakeStore) + Send + Sync + 'static) {
        *self
            .on_snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Box::new(hook));
    }

    pub fn head(&self) -> u64 {
        self.state().log.len() as u64
    }

    pub fn log(&self) -> Vec<SeqOp> {
        self.state().log.clone()
    }

    pub fn noted(&self) -> Vec<(DeviceId, ClientKind, u64)> {
        self.state().noted.clone()
    }

    pub fn snapshot_calls(&self) -> Vec<Scope> {
        self.state().snapshot_calls.clone()
    }

    pub fn current_page_set(&self, page: PageId) {
        self.state().current_page = Some(page);
    }

    pub fn current(&self) -> Option<PageId> {
        self.state().current_page
    }
}

impl SyncStore for FakeStore {
    async fn status(&self) -> Result<LogStatus, StoreError> {
        let st = self.state();
        if st.fail_status {
            return Err(StoreError("status down".to_owned()));
        }
        Ok(LogStatus {
            head_seq: st.log.len() as u64,
            log_floor: st.floor,
        })
    }

    fn server_id(&self) -> penwise_proto::ServerId {
        penwise_proto::ServerId::from_parts(1_700_000_000_000, 7)
    }

    async fn append_ops(
        &self,
        device: DeviceId,
        ops: Vec<AnyOp>,
    ) -> Result<Vec<u64>, AppendFailure> {
        let refused: Vec<(OpId, OpRejection)> = ops
            .iter()
            .filter_map(|op| {
                if op.device() == device {
                    op.validate().err().map(|r| (op.op_id(), r))
                } else {
                    Some((op.op_id(), OpRejection::NotOwnDevice))
                }
            })
            .collect();
        if !refused.is_empty() {
            return Err(AppendFailure::Rejected(refused));
        }
        let ids: Vec<OpId> = ops.iter().map(AnyOp::op_id).collect();
        let before = self.head();
        self.commit(ops);
        let st = self.state();
        let _ = before;
        Ok(ids
            .iter()
            .map(|id| {
                st.log
                    .iter()
                    .find(|e| e.op.op_id() == *id)
                    .map_or(0, |e| e.seq)
            })
            .collect())
    }

    async fn ops_since(
        &self,
        after: u64,
        filter: OpFilter,
        limit: usize,
    ) -> Result<OpsRead, StoreError> {
        let mut st = self.state();
        if after < st.floor || std::mem::take(&mut st.below_floor_once) {
            return Ok(OpsRead::BelowFloor);
        }
        let head = st.log.len() as u64;
        let rest = st.log.iter().filter(|e| e.seq > after);
        let page_ops: Vec<&SeqOp> = rest.take(limit).collect();
        let upto = if page_ops.len() == limit {
            page_ops.last().map_or(head, |e| e.seq)
        } else {
            head
        };
        let ops = page_ops
            .into_iter()
            .filter(|e| match filter {
                OpFilter::All => true,
                OpFilter::Catalog { page, .. } => match &e.op {
                    AnyOp::Item(i) => Some(i.page) == page,
                    _ => true,
                },
            })
            .cloned()
            .collect();
        Ok(OpsRead::Page(OpsPage {
            ops,
            upto_seq: upto,
            head_seq: head,
        }))
    }

    async fn snapshot(&self, scope: Scope) -> Result<SnapshotRead, StoreError> {
        let head_seq = {
            let mut st = self.state();
            st.snapshot_calls.push(scope);
            st.log.len() as u64
        };
        let hook = self
            .on_snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(hook) = hook {
            hook(self);
        }
        let (tx, rx) = mpsc::channel(4);
        tx.send(Ok(SnapshotChunk::default())).await.ok();
        Ok(SnapshotRead {
            head_seq,
            chunks: rx,
        })
    }

    fn subscribe(&self) -> broadcast::Receiver<Arc<[SeqOp]>> {
        self.tx.subscribe()
    }

    async fn note_device(
        &self,
        device: DeviceId,
        kind: ClientKind,
        acked_seq: u64,
    ) -> Result<(), StoreError> {
        self.state().noted.push((device, kind, acked_seq));
        Ok(())
    }

    async fn current_page(&self) -> Result<Option<PageId>, StoreError> {
        Ok(self.state().current_page)
    }

    async fn set_current_page(&self, page: PageId) -> Result<(), StoreError> {
        self.state().current_page = Some(page);
        Ok(())
    }
}
