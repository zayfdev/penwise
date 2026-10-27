//! Retention edge cases: purged ids never come back, restore reports what it cannot undo, retried ops whose
//! log rows were compacted away are recognized.

use penwise_core::edit::{Edit, ItemPatch};
use penwise_proto::{Background, NotebookChange, OpRejection, PageChange, PageKind, PagePatch};

use super::Rig;
use crate::storage::{AppendError, RETENTION_DAYS};

const DAY: u64 = 86_400_000;

#[tokio::test]
async fn an_add_for_a_purged_item_stays_deleted() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let item = rig.stroke(0);
    let id = item.id;
    let add = rig.item_op(page, Edit::Add(item.clone()));
    rig.push(vec![add.clone()]).await;
    rig.one(page, Edit::Delete(id)).await;
    rig.tick((RETENTION_DAYS + 1) * DAY);
    let report = rig.storage.compact().await.expect("compact");
    assert_eq!(report.items_purged, 1, "the tombstone is purged");

    // The same op again (a device offline for weeks) and a fresh op for the same id are both refused.
    let again = rig.storage.append_ops(rig.device, vec![add]).await;
    let Err(AppendError::Rejected(list)) = again else {
        panic!("expected a rejection, got {again:?}");
    };
    assert_eq!(list[0].reason, OpRejection::Duplicate);
    let fresh = rig.item_op(page, Edit::Add(item));
    assert_eq!(
        rig.rejected(vec![fresh]).await[0].reason,
        OpRejection::Duplicate
    );
    assert!(
        rig.storage
            .live_item_ids(page, None)
            .await
            .expect("live")
            .is_empty(),
        "the erased item stays gone"
    );
    // The memory survives a reopen.
    rig.reopen();
    let late = rig.stroke(1);
    let mut late = late;
    late.id = id;
    let op = rig.item_op(page, Edit::Add(late));
    assert_eq!(
        rig.rejected(vec![op]).await[0].reason,
        OpRejection::Duplicate
    );
}

#[tokio::test]
async fn restore_lists_items_it_cannot_roll_back() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let (old, young) = (rig.stroke(0), rig.stroke(1));
    let (old_id, young_id) = (old.id, young.id);
    rig.one(page, Edit::Add(old)).await; // seq 3, will be compacted
    rig.tick(RETENTION_DAYS * DAY + DAY);
    rig.one(page, Edit::Add(young)).await; // seq 4
    let checkpoint = rig.storage.status().await.expect("status").head_seq;
    rig.many(
        page,
        vec![
            Edit::Delete(old_id),
            Edit::Update {
                id: young_id,
                patch: ItemPatch {
                    z: Some(50),
                    ..ItemPatch::default()
                },
            },
        ],
    )
    .await;
    let floor = rig.storage.compact().await.expect("compact").log_floor;
    assert!(floor >= 2 && floor < checkpoint, "floor {floor}");
    let restored = rig
        .storage
        .restore_page(page, checkpoint)
        .await
        .expect("restore");
    assert_eq!(
        restored.skipped,
        vec![old_id],
        "the compacted item is reported"
    );
    assert_eq!(
        restored.appended.len(),
        1,
        "the retained item is rolled back"
    );
    let live = rig.storage.live_item_ids(page, None).await.expect("live");
    assert!(live.contains(&young_id) && !live.contains(&old_id));
}

#[tokio::test]
async fn retried_ops_after_compaction_are_recognized_not_refused_or_relogged() {
    let mut rig = Rig::new();
    let notebook = rig.ids.notebook();
    let page = rig.ids.page();
    let create_notebook = rig.notebook_op(
        NotebookChange::Create {
            title: "nb".into(),
            position: 0,
        },
        notebook,
    );
    let create_page = rig.page_op(
        page,
        PageChange::Create {
            notebook,
            title: "p".into(),
            position: 0,
            kind: PageKind::Infinite,
            background: Background::Blank,
        },
    );
    let rename = rig.page_op(
        page,
        PageChange::Update(PagePatch {
            title: Some("renamed".into()),
            ..PagePatch::default()
        }),
    );
    let item = rig.stroke(0);
    let id = item.id;
    let add = rig.item_op(page, Edit::Add(item));
    let nudge = rig.item_op(
        page,
        Edit::Update {
            id,
            patch: ItemPatch {
                z: Some(7),
                ..ItemPatch::default()
            },
        },
    );
    let frame = vec![create_notebook, create_page, rename, add, nudge];
    rig.push(frame.clone()).await;
    let head = rig.storage.status().await.expect("status").head_seq;
    assert_eq!(head, 5);
    rig.tick((RETENTION_DAYS + 1) * DAY);
    let floor = rig.storage.compact().await.expect("compact").log_floor;
    assert_eq!(floor, head, "every log row is gone");

    let retried = rig.push(frame).await;
    assert!(
        retried.iter().all(|a| a.duplicate),
        "every retried op is recognized: {retried:?}"
    );
    assert!(retried.iter().all(|a| a.seq <= floor));
    assert_eq!(
        rig.storage.status().await.expect("status").head_seq,
        head,
        "nothing was logged again"
    );
}

#[tokio::test]
async fn bootstrap_writes_every_identity_row_together() {
    let rig = Rig::new();
    let conn = rusqlite::Connection::open(rig.dir.path().join("penwise.db")).expect("open");
    for key in [
        "server_id",
        "server_device",
        "created_ms",
        "log_floor",
        "retention_days",
    ] {
        let present: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM server_meta WHERE key = ?1",
                [key],
                |r| r.get::<_, i64>(0),
            )
            .expect("count")
            == 1;
        assert!(present, "{key} written at bootstrap");
    }
}

#[tokio::test]
async fn an_op_that_could_not_be_delivered_in_one_frame_is_refused() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let id = rig.item_id();
    let mut huge =
        penwise_testkit::stroke_item(id, 0, penwise_core::item::Layer::Ink, 1, &[[0.0, 0.0]]);
    if let penwise_core::item::ItemKind::Stroke(stroke) = &mut huge.kind {
        stroke.samples = (0..1_000_000_u32)
            .map(|i| {
                penwise_core::Sample {
                    x: (i % 1000) as f32,
                    y: (i / 1000) as f32,
                    pressure: 0.5,
                    tilt: 0.0,
                    azimuth: 0.0,
                    t_us: i / 8,
                }
                .quantize()
            })
            .collect();
    }
    let op = rig.item_op(page, Edit::Add(huge));
    let size = penwise_proto::encode_op(&op).map_or(usize::MAX, |b| b.len());
    assert!(
        size > penwise_proto::outbox::MAX_OP_BYTES,
        "fixture is {size} bytes"
    );
    assert_eq!(rig.rejected(vec![op]).await[0].reason, OpRejection::Invalid);
    assert_eq!(rig.storage.status().await.expect("status").head_seq, 2);
}
