//! Writer behaviour: idempotence, reference rules, atomic frames, gap-free seq, broadcast.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test fixtures: small counts and coordinates"
)]

mod compaction;
mod lifecycle;
mod lww;
mod purge;
mod rounds;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use penwise_core::edit::Edit;
use penwise_core::item::{Item, ItemId, Layer};
use penwise_proto::{
    AnyOp, Background, DeviceId, HlcClock, ItemRef, NotebookChange, NotebookId, NotebookOp, Op,
    OpRejection, PageChange, PageId, PageKind, PageOp,
};
use penwise_testkit::{IdGen, TempDir, stroke_item};

use super::{AppendError, FaultPlan, Rejected, Storage, StorageConfig, StorageError};

pub(super) const T0: u64 = 1_800_000_000_000;

/// A storage in a temp dir with a settable clock and one authoring device.
pub(super) struct Rig {
    pub dir: TempDir,
    pub storage: Storage,
    pub now: Arc<AtomicU64>,
    pub ids: IdGen,
    pub device: DeviceId,
    pub clock: HlcClock,
    next_item: u128,
}

impl Rig {
    pub fn new() -> Self {
        Self::with_fault(None)
    }

    pub fn with_fault(fault: Option<FaultPlan>) -> Self {
        let dir = TempDir::new("storage").expect("tempdir");
        let now = Arc::new(AtomicU64::new(T0));
        let storage = open(&dir, &now, fault);
        let mut ids = IdGen::new(7);
        let device = ids.device();
        Self {
            dir,
            storage,
            now,
            ids,
            device,
            clock: HlcClock::new(device),
            next_item: 1,
        }
    }

    pub fn reopen(&mut self) {
        self.storage = open(&self.dir, &self.now, None);
    }

    pub fn tick(&mut self, ms: u64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }

    pub fn hlc(&mut self) -> penwise_proto::Hlc {
        self.clock.send(self.now.load(Ordering::SeqCst))
    }

    pub fn notebook_op(&mut self, change: NotebookChange, id: NotebookId) -> AnyOp {
        AnyOp::Notebook(NotebookOp {
            op_id: self.ids.op(),
            device: self.device,
            hlc: self.hlc(),
            notebook: id,
            change,
        })
    }

    pub fn page_op(&mut self, page: PageId, change: PageChange) -> AnyOp {
        AnyOp::Page(PageOp {
            op_id: self.ids.op(),
            device: self.device,
            hlc: self.hlc(),
            page,
            change,
        })
    }

    pub fn item_op(&mut self, page: PageId, edit: Edit) -> AnyOp {
        AnyOp::Item(Box::new(Op {
            op_id: self.ids.op(),
            device: self.device,
            hlc: self.hlc(),
            page,
            edit,
        }))
    }

    pub fn item_id(&mut self) -> ItemId {
        self.next_item += 1;
        ItemRef::from_parts(1, self.next_item).into()
    }

    pub fn stroke(&mut self, z: i64) -> Item {
        let id = self.item_id();
        stroke_item(
            id,
            z,
            Layer::Ink,
            5,
            &[[1.0, 1.0], [30.0, 30.0], [60.0, 20.0]],
        )
    }

    /// Creates a notebook and a page; returns `(notebook, page)`.
    pub async fn seed_page(&mut self) -> (NotebookId, PageId) {
        let notebook = self.ids.notebook();
        let page = self.ids.page();
        let ops = vec![
            self.notebook_op(
                NotebookChange::Create {
                    title: "nb".into(),
                    position: 0,
                },
                notebook,
            ),
            self.page_op(
                page,
                PageChange::Create {
                    notebook,
                    title: "p".into(),
                    position: 0,
                    kind: PageKind::Infinite,
                    background: Background::Blank,
                },
            ),
        ];
        self.push(ops).await;
        (notebook, page)
    }

    pub async fn push(&self, ops: Vec<AnyOp>) -> Vec<super::Appended> {
        self.storage
            .append_ops(self.device, ops)
            .await
            .expect("append")
    }

    /// Pushes one item edit.
    pub async fn one(&mut self, page: PageId, edit: Edit) -> Vec<super::Appended> {
        let op = self.item_op(page, edit);
        self.push(vec![op]).await
    }

    /// Pushes several item edits as one frame.
    pub async fn many(&mut self, page: PageId, edits: Vec<Edit>) -> Vec<super::Appended> {
        let ops = edits.into_iter().map(|e| self.item_op(page, e)).collect();
        self.push(ops).await
    }

    pub async fn rejected(&self, ops: Vec<AnyOp>) -> Vec<Rejected> {
        match self.storage.append_ops(self.device, ops).await {
            Err(AppendError::Rejected(list)) => list,
            other => panic!("expected rejection, got {other:?}"),
        }
    }
}

fn open(dir: &TempDir, now: &Arc<AtomicU64>, fault: Option<FaultPlan>) -> Storage {
    let clock = Arc::clone(now);
    let mut config = StorageConfig::new(dir.path().join("penwise.db"));
    config.now_ms = Arc::new(move || clock.load(Ordering::SeqCst));
    config.fault = fault;
    Storage::open(config).expect("open")
}

#[tokio::test]
async fn re_pushing_a_frame_returns_the_original_seqs_and_broadcasts_once() {
    let mut rig = Rig::new();
    let mut feed = rig.storage.subscribe();
    let (_, page) = rig.seed_page().await;
    let item = rig.stroke(0);
    let frame = vec![rig.item_op(page, Edit::Add(item))];
    let first = rig.push(frame.clone()).await;
    let again = rig.push(frame).await;
    assert_eq!(first[0].seq, 3);
    assert!(!first[0].duplicate);
    assert_eq!((again[0].seq, again[0].duplicate), (3, true));
    assert_eq!(rig.storage.status().await.expect("status").head_seq, 3);
    let mut seen = Vec::new();
    while let Ok(batch) = feed.try_recv() {
        seen.extend(batch.iter().map(|op| op.seq));
    }
    assert_eq!(seen, vec![1, 2, 3]);
}

#[tokio::test]
async fn references_must_exist_before_they_are_used() {
    let mut rig = Rig::new();
    let (notebook, page) = rig.seed_page().await;
    let stranger = rig.ids.page();
    let lone = rig.stroke(0);
    let ghost = rig.item_id();
    let real = rig.stroke(0);
    let real_id = real.id;
    rig.one(page, Edit::Add(real)).await;
    let other_page = rig.ids.page();
    let create = rig.page_op(
        other_page,
        PageChange::Create {
            notebook,
            title: String::new(),
            position: 1,
            kind: PageKind::Infinite,
            background: Background::Grid,
        },
    );
    rig.push(vec![create]).await;
    let (stray_page, stray_notebook) = (rig.ids.page(), rig.ids.notebook());
    let bad_notebook = rig.page_op(
        stray_page,
        PageChange::Create {
            notebook: stray_notebook,
            title: String::new(),
            position: 0,
            kind: PageKind::Infinite,
            background: Background::Blank,
        },
    );
    let cases = [
        (
            rig.item_op(stranger, Edit::Add(lone)),
            OpRejection::UnknownPage,
        ),
        (
            rig.item_op(page, Edit::Delete(ghost)),
            OpRejection::UnknownItem,
        ),
        (
            rig.item_op(other_page, Edit::Delete(real_id)),
            OpRejection::WrongPage,
        ),
        (bad_notebook, OpRejection::UnknownNotebook),
    ];
    for (op, expected) in cases {
        let refused = rig.rejected(vec![op]).await;
        assert_eq!(refused[0].reason, expected);
    }
    // A second Add of an existing id under another op id is a duplicate.
    let twin = stroke_item(real_id, 0, Layer::Ink, 9, &[[0.0, 0.0], [5.0, 5.0]]);
    let twin = rig.item_op(page, Edit::Add(twin));
    let refused = rig.rejected(vec![twin]).await;
    assert_eq!(refused[0].reason, OpRejection::Duplicate);
}

#[tokio::test]
async fn foreign_device_and_far_future_clock_are_refused() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let stroke = rig.stroke(0);
    let mut foreign = rig.item_op(page, Edit::Add(stroke));
    if let AnyOp::Item(op) = &mut foreign {
        op.device = rig.ids.device();
    }
    assert_eq!(
        rig.rejected(vec![foreign]).await[0].reason,
        OpRejection::NotOwnDevice
    );
    rig.tick(0);
    let stroke = rig.stroke(0);
    let mut future = rig.item_op(page, Edit::Add(stroke));
    if let AnyOp::Item(op) = &mut future {
        op.hlc.wall_ms = T0 + 10 * 60 * 1000;
    }
    assert_eq!(
        rig.rejected(vec![future]).await[0].reason,
        OpRejection::ClockSkew
    );
}

#[tokio::test]
async fn a_frame_with_one_bad_op_logs_nothing() {
    let mut rig = Rig::new();
    let mut feed = rig.storage.subscribe();
    let (_, page) = rig.seed_page().await;
    while feed.try_recv().is_ok() {}
    let (a, b) = (rig.stroke(0), rig.stroke(1));
    let ghost = rig.item_id();
    let bad = rig.item_op(page, Edit::Delete(ghost));
    let first = rig.item_op(page, Edit::Add(a.clone()));
    let last = rig.item_op(page, Edit::Add(b));
    let frame = vec![first, bad, last];
    let refused = rig.rejected(frame).await;
    assert_eq!(refused.len(), 1);
    assert_eq!(rig.storage.status().await.expect("status").head_seq, 2);
    assert!(
        rig.storage
            .page_view(page)
            .await
            .expect("view")
            .expect("page")
            .replica
            .items()
            .next()
            .is_none()
    );
    assert!(feed.try_recv().is_err());
    // The good op still goes in afterwards, with the next seq (no gap).
    let ok = rig.one(page, Edit::Add(a)).await;
    assert_eq!(ok[0].seq, 3);
}

#[tokio::test]
async fn an_injected_commit_fault_rolls_back_and_leaves_no_seq_gap() {
    let mut rig = Rig::with_fault(Some(FaultPlan::new(3)));
    let (_, page) = rig.seed_page().await;
    let mut feed = rig.storage.subscribe();
    let ops: Vec<AnyOp> = (0..3)
        .map(|z| {
            let item = rig.stroke(z);
            rig.item_op(page, Edit::Add(item))
        })
        .collect();
    let error = rig
        .storage
        .append_ops(rig.device, ops.clone())
        .await
        .expect_err("fault");
    assert!(matches!(error, AppendError::Storage(StorageError::Fault)));
    assert_eq!(rig.storage.status().await.expect("status").head_seq, 2);
    assert!(feed.try_recv().is_err());
    let retried = rig.push(ops).await;
    assert_eq!(
        retried.iter().map(|a| a.seq).collect::<Vec<_>>(),
        vec![3, 4, 5]
    );
}

#[tokio::test]
async fn concurrent_pushers_get_gap_free_unique_seqs() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let mut tasks = Vec::new();
    for worker in 0..8_u64 {
        let storage = rig.storage.clone();
        let device = rig.ids.device();
        let mut clock = HlcClock::new(device);
        let mut ids = IdGen::new(100 + worker);
        tasks.push(tokio::spawn(async move {
            let mut seqs = Vec::new();
            for n in 0..20_i64 {
                let item = stroke_item(
                    ItemRef::from_parts(2 + worker, u128::from(worker) << 32 | n as u128).into(),
                    n,
                    Layer::Ink,
                    1,
                    &[[0.0, 0.0], [9.0, 9.0]],
                );
                let op = AnyOp::Item(Box::new(Op {
                    op_id: ids.op(),
                    device,
                    hlc: clock.send(T0),
                    page,
                    edit: Edit::Add(item),
                }));
                let done = storage.append_ops(device, vec![op]).await.expect("append");
                seqs.push(done[0].seq);
            }
            seqs
        }));
    }
    let mut all = Vec::new();
    for task in tasks {
        all.extend(task.await.expect("join"));
    }
    all.sort_unstable();
    assert_eq!(all, (3..=162).collect::<Vec<u64>>());
    let page = rig
        .storage
        .ops_since(0, super::OpFilter::All, 1000)
        .await
        .expect("ops");
    let seqs: Vec<u64> = page.ops.iter().map(|op| op.seq).collect();
    assert_eq!(seqs, (1..=162).collect::<Vec<u64>>());
}
