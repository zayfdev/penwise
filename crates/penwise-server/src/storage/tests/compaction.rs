//! Retention in slices, and who may hold the floor back.

use penwise_core::edit::Edit;
use penwise_proto::ClientKind;

use super::{Rig, T0};
use crate::storage::compact::{self, BATCH_ROWS};
use crate::storage::{DeviceInfo, RETENTION_DAYS, schema};

const DAY: u64 = 86_400_000;

#[tokio::test]
async fn a_recently_seen_browser_never_holds_the_floor_back() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let browser = DeviceInfo {
        id: rig.ids.device(),
        name: "viewer".into(),
        kind: ClientKind::Browser,
    };
    // A browser connected at the very start and was seen again just now, with a cursor of 0.
    rig.storage
        .touch_device(browser.clone(), 0)
        .await
        .expect("touch");
    let item = rig.stroke(0);
    rig.one(page, Edit::Add(item)).await;
    rig.tick((RETENTION_DAYS + 1) * DAY);
    rig.storage.touch_device(browser, 0).await.expect("touch");
    let report = rig.storage.compact().await.expect("compact");
    assert_eq!(
        report.log_floor, 3,
        "the old log is compacted regardless of the browser"
    );
}

#[tokio::test]
async fn an_active_tablet_holds_the_floor_at_its_ack() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let tablet = DeviceInfo {
        id: rig.ids.device(),
        name: "tablet".into(),
        kind: ClientKind::Tablet,
    };
    rig.storage
        .touch_device(tablet.clone(), 1)
        .await
        .expect("touch");
    let item = rig.stroke(0);
    rig.one(page, Edit::Add(item)).await;
    rig.tick((RETENTION_DAYS + 1) * DAY);
    // Kept alive by a periodic ack bump: it still counts, and holds the floor at its ack.
    rig.storage.touch_device(tablet, 1).await.expect("touch");
    assert_eq!(rig.storage.compact().await.expect("compact").log_floor, 1);
}

#[test]
fn compaction_deletes_in_bounded_slices_and_raises_the_floor_first() {
    let dir = penwise_testkit::TempDir::new("compact-steps").expect("tempdir");
    let mut conn = schema::open_writer(&dir.path().join("penwise.db")).expect("open");
    schema::ensure_current(&mut conn).expect("schema");
    let total = BATCH_ROWS * 2 + 7;
    conn.execute(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?1)
         INSERT INTO ops (op_id, batch, device, hlc, kind, body, received_ms)
         SELECT randomblob(16), 1, zeroblob(16), zeroblob(16), 0, x'00', 0 FROM n",
        [i64::try_from(total).expect("count")],
    )
    .expect("fill");
    let now = T0 + (RETENTION_DAYS + 1) * DAY;

    let first = compact::step(&mut conn, now).expect("step");
    assert_eq!(first.report.ops_deleted, BATCH_ROWS as u64);
    assert!(first.more);
    assert_eq!(
        first.report.log_floor, total as u64,
        "the floor moves with the first slice"
    );
    let second = compact::step(&mut conn, now).expect("step");
    assert_eq!(second.report.ops_deleted, BATCH_ROWS as u64);
    assert!(second.more);
    let third = compact::step(&mut conn, now).expect("step");
    assert_eq!((third.report.ops_deleted, third.more), (7, false));
    let left: i64 = conn
        .query_row("SELECT COUNT(*) FROM ops", [], |r| r.get(0))
        .expect("count");
    assert_eq!(left, 0);
}
