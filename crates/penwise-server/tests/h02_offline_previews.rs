//! H-2: offline erase, then ~60 s of live frames while offline -> nothing queued but real ops; the erase survives and
//! is delivered exactly once. Plus the online side of the live channel (watchers only, never echoed).
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
use penwise_testkit::check;

#[tokio::test]
async fn offline_previews_are_never_queued_and_the_erase_survives() {
    let server = TestServer::start().await;
    let mut a = server.tablet(1);
    let mut b = server.tablet(2);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "P");
    a.draw(page, &line(40.0, 100.0, 60));
    converge(&server, &mut [&mut a, &mut b]).await;
    b.watch(Some(page)).await.expect("watch");

    a.offline();
    let erased = a.erase(page, &[[100.0, 60.0], [100.0, 140.0]], 6.0);
    assert!(erased > 0, "eraser must hit");
    let queued = a.outbox.len();
    assert_eq!(queued, erased, "only the erase ops are queued");

    // 60 s of pen input at 60 Hz while offline: every preview frame fails, none is buffered.
    let stroke = a.ids.next_id_item();
    let mut refused = 0;
    for frame in 0_u16..3600 {
        let start = u32::from(frame) * 2;
        let pts = line(10.0 + f32::from(frame % 50), 300.0, 2);
        if a.send_live(page, stroke, start, &pts).await.is_err() {
            refused += 1;
        }
    }
    assert_eq!(refused, 3600, "offline sends must fail instead of queueing");
    assert_eq!(a.outbox.len(), queued, "previews must not enter the outbox");

    let before = check::db_seqs(&server.db_path()).expect("seqs").len();
    a.online().await.expect("reconnect");
    b.pump_quiet(Duration::from_millis(300)).await;
    let after = check::db_seqs(&server.db_path()).expect("seqs").len();
    assert_eq!(
        after - before,
        queued,
        "each offline op is logged exactly once"
    );
    assert!(
        b.events.deltas.is_empty(),
        "no stale previews reach watchers: {}",
        b.events.deltas.len()
    );
    let mut delivered = b.events.delivered.clone();
    delivered.dedup();
    assert_eq!(delivered, b.events.delivered, "B receives each seq once");
    converge(&server, &mut [&mut a, &mut b]).await;
}

#[tokio::test]
async fn live_deltas_reach_watchers_but_never_the_producer() {
    let server = TestServer::start().await;
    let mut a = server.tablet(1);
    let mut b = server.tablet(2);
    let mut c = server.tablet(3);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "P");
    let other = a.create_page(nb, "Q");
    converge(&server, &mut [&mut a, &mut b, &mut c]).await;
    b.watch(Some(page)).await.expect("watch");
    c.watch(Some(other)).await.expect("watch other");
    a.watch(Some(page)).await.expect("producer watches too");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let stroke = a.ids.next_id_item();
    a.send_live(page, stroke, 0, &line(10.0, 10.0, 5))
        .await
        .expect("delta 1");
    a.send_live(page, stroke, 5, &line(40.0, 10.0, 5))
        .await
        .expect("delta 2");
    assert!(
        b.pump_until(Duration::from_secs(5), |d| d.events.deltas.len() >= 2)
            .await,
        "watcher gets both deltas"
    );
    assert_eq!(
        b.events.deltas[0].0, a.id,
        "deltas carry the producer's device id"
    );
    a.pump_quiet(Duration::from_millis(200)).await;
    c.pump_quiet(Duration::from_millis(200)).await;
    assert!(a.events.deltas.is_empty(), "never echoed to the producer");
    assert!(
        c.events.deltas.is_empty(),
        "a watcher of another page gets nothing"
    );

    a.cancel_live(page, stroke).await.expect("cancel");
    assert!(
        b.pump_until(Duration::from_secs(5), |d| !d.events.cancels.is_empty())
            .await,
        "cancel is forwarded"
    );
}
