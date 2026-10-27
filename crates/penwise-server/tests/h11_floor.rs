//! H-11: a client behind `log_floor`, or talking to a server whose DB was swapped (different `ServerId`), receives a
//! snapshot; its local outbox is preserved and re-pushed.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::similar_names,
    clippy::items_after_statements
)]

mod common;

use std::time::Duration;

use common::{TestServer, converge, line};

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

#[tokio::test]
async fn a_client_behind_the_floor_gets_a_snapshot_and_keeps_its_outbox() {
    let server = TestServer::start().await;
    let mut a = server.tablet(1);
    let mut b = server.tablet(2);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "P");
    a.draw(page, &line(10.0, 10.0, 20));
    converge(&server, &mut [&mut a, &mut b]).await;
    let b_seen = b.replica.head_seq();
    b.offline();

    // A keeps writing while B is away; then B is unseen for 31 days (stops holding the floor) and the
    // 8-day-old log is compacted.
    for i in 0..4_u16 {
        a.draw(page, &line(60.0, 30.0 + f32::from(i) * 20.0, 12));
    }
    a.settle(Duration::from_secs(10)).await.expect("a settles");
    let head = server.head();
    // A's cursor must cover its own pushes: the server's floor follows the cursor a device reports in `Hello`.
    assert!(
        a.pump_until(Duration::from_secs(10), |d| d.replica.head_seq() >= head)
            .await
    );
    server.advance(31 * DAY_MS);
    a.offline();
    a.online().await.expect("a refreshes its last-seen");
    a.draw(page, &line(200.0, 10.0, 8));
    a.settle(Duration::from_secs(10))
        .await
        .expect("a settles again");
    server.storage.compact().await.expect("compact");
    let status = server.storage.status().await.expect("status");
    assert!(
        status.log_floor > b_seen,
        "compaction must raise the floor above B's cursor: {status:?} vs {b_seen}"
    );

    // B drew something offline too.
    let mine = b.draw(page, &line(300.0, 300.0, 10));
    b.online().await.expect("b reconnects");
    assert_eq!(
        b.events.snapshots, 1,
        "behind the floor -> snapshot, never a partial diff"
    );
    assert!(b.outbox.dead().is_empty());
    let fp = converge(&server, &mut [&mut a, &mut b]).await;
    assert!(
        fp.items.keys().any(|(_, i)| *i == mine),
        "B's offline stroke was re-pushed after the snapshot"
    );
}

#[tokio::test]
async fn a_swapped_server_db_means_snapshot_and_the_outbox_is_repushed() {
    let mut server = TestServer::start().await;
    let mut a = server.tablet(1);
    let nb = a.create_notebook("Old");
    let old_page = a.create_page(nb, "Old page");
    a.draw(old_page, &line(10.0, 10.0, 12));
    converge(&server, &mut [&mut a]).await;
    let old_server = a.pinned;
    a.offline();

    // Offline: one self-contained notebook/page/stroke, and one stroke on a page the new server will not know.
    let nb2 = a.create_notebook("New");
    let page2 = a.create_page(nb2, "New page");
    let fresh = a.draw(page2, &line(20.0, 20.0, 12));
    let orphan = a.draw(old_page, &line(30.0, 30.0, 12));

    server.restart_on_fresh_db().await;
    a.online().await.expect("reconnect to the swapped server");
    assert_ne!(a.pinned, old_server, "the client noticed the new ServerId");
    assert_eq!(server.storage.server_id(), a.pinned.expect("pinned"));
    let fp = server.fingerprint();
    assert!(
        fp.items.keys().any(|(_, i)| *i == fresh),
        "self-contained offline work is re-pushed"
    );
    assert!(
        !fp.items.keys().any(|(_, i)| *i == orphan),
        "a stroke on an unknown page cannot land"
    );
    assert!(
        a.outbox.dead().iter().any(|d| d.bytes.len() > 1),
        "...but it is kept as a dead letter, never silently dropped: {:?}",
        a.outbox.dead().len()
    );
    assert!(a.outbox.is_empty());
}
