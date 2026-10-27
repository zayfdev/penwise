//! Reopen, retention, restore, snapshots, filters.

use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::item::Layer;
use penwise_proto::{AnyOp, PageChange, PageOp, PagePatch, Scope, SnapshotChunk};

use super::{Rig, T0};
use crate::storage::{
    DEVICE_IDLE_DAYS, DeviceInfo, OpFilter, RETENTION_DAYS, RestoreError, StorageError,
};

const DAY: u64 = 86_400_000;

#[tokio::test]
async fn identity_and_server_clock_survive_a_reopen() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let id = rig.storage.server_id();
    let device = rig.storage.server_device();
    // A device clock calibrated ahead of the server by 4 minutes (inside the skew limit).
    rig.tick(4 * 60 * 1000);
    let item = rig.stroke(0);
    rig.one(page, Edit::Add(item)).await;
    let before = rig.storage.server_stamp().await.expect("stamp");
    let renamed = AnyOp::Page(PageOp {
        op_id: rig.ids.op(),
        device,
        hlc: before,
        page,
        change: PageChange::Update(PagePatch {
            title: Some("by server".into()),
            ..PagePatch::default()
        }),
    });
    rig.storage
        .append_ops(device, vec![renamed])
        .await
        .expect("server op");
    rig.reopen();
    assert_eq!(rig.storage.server_id(), id);
    assert_eq!(rig.storage.server_device(), device);
    let after = rig.storage.server_stamp().await.expect("stamp");
    assert!(after > before, "{after:?} must exceed {before:?}");
    assert_eq!(after.device, device);
}

#[tokio::test]
async fn ops_since_filters_by_scope_and_reports_upto() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let (_, other) = rig.seed_page().await;
    for target in [page, other, page] {
        let item = rig.stroke(0);
        rig.one(target, Edit::Add(item)).await;
    }
    let all = rig
        .storage
        .ops_since(0, OpFilter::All, 100)
        .await
        .expect("all");
    assert_eq!(all.ops.len(), 7);
    assert_eq!((all.upto_seq, all.head_seq, all.more), (7, 7, false));
    let watched = rig
        .storage
        .ops_since(0, OpFilter::CatalogAnd(Some(page)), 100)
        .await
        .expect("watched");
    let seqs: Vec<u64> = watched.ops.iter().map(|o| o.seq).collect();
    assert_eq!(seqs, vec![1, 2, 3, 4, 5, 7]);
    assert_eq!(watched.upto_seq, 7);
    let catalog_only = rig
        .storage
        .ops_since(0, OpFilter::CatalogAnd(None), 100)
        .await
        .expect("cat");
    assert_eq!(catalog_only.ops.len(), 4);
    let paged = rig
        .storage
        .ops_since(2, OpFilter::All, 3)
        .await
        .expect("paged");
    assert_eq!((paged.upto_seq, paged.more, paged.ops.len()), (5, true, 3));
}

#[tokio::test]
async fn snapshot_streams_every_row_at_one_head() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let ops: Vec<_> = (0..600)
        .map(|z| {
            let item = rig.stroke(z);
            rig.item_op(page, Edit::Add(item))
        })
        .collect();
    rig.push(ops).await;
    let mut stream = rig.storage.snapshot(Scope::All).await.expect("snapshot");
    assert_eq!(stream.head_seq, 602);
    let mut chunks: Vec<SnapshotChunk> = Vec::new();
    while let Some(chunk) = stream.chunks.recv().await {
        chunks.push(chunk.expect("chunk"));
    }
    assert_eq!(chunks[0].notebooks.len(), 1);
    assert_eq!(chunks[0].pages.len(), 1);
    let items: usize = chunks.iter().map(|c| c.items.len()).sum();
    assert_eq!(items, 600);
    assert!(chunks.iter().all(|c| c.items.len() <= 512));
    let mut page_only = rig.storage.snapshot(Scope::Page(page)).await.expect("page");
    let first = page_only.chunks.recv().await.expect("chunk").expect("ok");
    assert!(first.notebooks.is_empty() && !first.items.is_empty());
}

#[tokio::test]
async fn compaction_keeps_what_active_devices_still_need() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let tablet = DeviceInfo {
        id: rig.ids.device(),
        name: "tablet".into(),
        kind: penwise_proto::ClientKind::Tablet,
    };
    let mut doomed = None;
    for z in 0..4 {
        let item = rig.stroke(z);
        if z == 0 {
            doomed = Some(item.id);
        }
        rig.one(page, Edit::Add(item)).await;
    }
    let doomed = doomed.expect("item");
    rig.one(page, Edit::Delete(doomed)).await;
    rig.storage
        .touch_device(tablet.clone(), 3)
        .await
        .expect("touch");
    rig.tick((RETENTION_DAYS + 1) * DAY);
    // The tablet is active and has only acked seq 3: the floor stops there.
    rig.storage
        .touch_device(tablet.clone(), 3)
        .await
        .expect("touch");
    let report = rig.storage.compact().await.expect("compact");
    assert_eq!(report.log_floor, 3);
    assert_eq!(report.ops_deleted, 3);
    let status = rig.storage.status().await.expect("status");
    assert_eq!((status.head_seq, status.log_floor), (7, 3));
    let behind = rig.storage.ops_since(2, OpFilter::All, 10).await;
    assert!(matches!(behind, Err(StorageError::BelowFloor { floor: 3 })));
    assert_eq!(
        rig.storage
            .ops_since(3, OpFilter::All, 10)
            .await
            .expect("tail")
            .ops
            .len(),
        4
    );
    // The tombstone's last write (seq 7) is above the floor, so it is still there.
    assert!(
        rig.storage
            .live_item_ids(page, None)
            .await
            .expect("ids")
            .len()
            == 3
    );

    // Acked everything: floor reaches head, the old tombstone is purged.
    rig.storage.touch_device(tablet, 7).await.expect("touch");
    let report = rig.storage.compact().await.expect("compact");
    assert_eq!((report.log_floor, report.items_purged), (7, 1));
    let view = rig
        .storage
        .page_view(page)
        .await
        .expect("view")
        .expect("page");
    assert!(view.replica.item(doomed).is_none());
}

#[tokio::test]
async fn a_device_unseen_for_thirty_days_does_not_hold_the_floor() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let stale = DeviceInfo {
        id: rig.ids.device(),
        name: "old tablet".into(),
        kind: penwise_proto::ClientKind::Tablet,
    };
    rig.storage.touch_device(stale, 0).await.expect("touch");
    let item = rig.stroke(0);
    rig.one(page, Edit::Add(item)).await;
    rig.tick((DEVICE_IDLE_DAYS + 1) * DAY);
    let report = rig.storage.compact().await.expect("compact");
    assert_eq!(report.log_floor, 3);
    // Retention also applies: ops newer than 7 days stay.
    let fresh = rig.stroke(1);
    rig.one(page, Edit::Add(fresh)).await;
    rig.tick(DAY);
    assert_eq!(rig.storage.compact().await.expect("compact").log_floor, 3);
}

#[tokio::test]
async fn restore_is_a_compensating_batch_and_can_be_undone() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let (keep, gone, late) = (rig.stroke(0), rig.stroke(1), rig.stroke(2));
    let (keep_id, gone_id, late_id) = (keep.id, gone.id, late.id);
    rig.many(page, vec![Edit::Add(keep), Edit::Add(gone)]).await;
    let checkpoint = rig.storage.status().await.expect("status").head_seq;
    rig.many(
        page,
        vec![
            Edit::Add(late),
            Edit::Delete(gone_id),
            Edit::Update {
                id: keep_id,
                patch: ItemPatch {
                    layer: Some(Layer::Agent),
                    z: Some(99),
                    ..ItemPatch::default()
                },
            },
        ],
    )
    .await;
    let restored = rig
        .storage
        .restore_page(page, checkpoint)
        .await
        .expect("restore");
    assert_eq!(restored.len(), 3);
    let live = rig.storage.live_item_ids(page, None).await.expect("live");
    assert!(live.contains(&keep_id) && live.contains(&gone_id) && !live.contains(&late_id));
    let view = rig
        .storage
        .page_view(page)
        .await
        .expect("view")
        .expect("page");
    let kept = view.replica.item(keep_id).expect("kept");
    assert_eq!((kept.item.layer, kept.item.z), (Layer::Ink, 0));
    // The restore is itself logged: restoring to before it brings the later state back.
    let head = rig.storage.status().await.expect("status").head_seq;
    assert_eq!(head, checkpoint + 3 + 3);
    let undo = rig
        .storage
        .restore_page(page, checkpoint + 3)
        .await
        .expect("undo");
    assert_eq!(undo.len(), 3);
    let live = rig.storage.live_item_ids(page, None).await.expect("live");
    assert!(!live.contains(&gone_id) && live.contains(&late_id));
    // Nothing differs now: an empty batch.
    assert!(
        rig.storage
            .restore_page(page, checkpoint + 3)
            .await
            .expect("noop")
            .is_empty()
    );
}

#[tokio::test]
async fn restore_reports_unknown_pages_and_compacted_history() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let ghost = rig.ids.page();
    assert!(matches!(
        rig.storage.restore_page(ghost, 1).await,
        Err(RestoreError::UnknownPage)
    ));
    let item = rig.stroke(0);
    rig.one(page, Edit::Add(item)).await;
    rig.tick((RETENTION_DAYS + 1) * DAY);
    rig.storage.compact().await.expect("compact");
    assert!(matches!(
        rig.storage.restore_page(page, 1).await,
        Err(RestoreError::BelowFloor { floor: 3 })
    ));
    let _ = T0;
}
