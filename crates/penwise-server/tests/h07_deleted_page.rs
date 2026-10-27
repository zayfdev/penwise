//! H-7: a page is deleted on A while B offline-adds; B reconnects -> the items are kept under the tombstoned page and
//! nothing loops (no resurrection, no rejection, no dead letters).
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

#[tokio::test]
async fn offline_ink_survives_a_deleted_page_without_a_resurrection_loop() {
    let server = TestServer::start().await;
    let mut a = server.tablet(1);
    let mut b = server.tablet(2);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "P");
    a.draw(page, &line(10.0, 10.0, 20));
    converge(&server, &mut [&mut a, &mut b]).await;

    b.offline();
    let kept: Vec<_> = (0..3_u16)
        .map(|i| b.draw(page, &line(50.0, 50.0 + f32::from(i) * 30.0, 25)))
        .collect();
    a.delete_page(page);
    a.settle(Duration::from_secs(10)).await.expect("a settles");
    let head_after_delete = server.head();

    b.online().await.expect("b reconnects");
    assert!(
        b.events.rejected.is_empty(),
        "ink is never rejected for lifecycle reasons: {:?}",
        b.events.rejected
    );
    assert!(b.outbox.dead().is_empty(), "nothing dead-lettered");
    let fp = converge(&server, &mut [&mut a, &mut b]).await;

    for id in &kept {
        assert!(
            fp.items.keys().any(|(p, i)| *p == page && i == id),
            "item {id:?} kept under the tombstoned page"
        );
    }
    assert_eq!(fp.live_on(page), 1 + kept.len());
    assert!(
        a.replica
            .catalog()
            .deleted_pages(nb)
            .iter()
            .any(|p| p.id == page),
        "the page stays tombstoned and appears in the deleted list"
    );
    assert!(
        a.replica.catalog().pages(nb).iter().all(|p| p.id != page),
        "no resurrection into the live list"
    );

    // Quiet afterwards: no ping-pong of ops.
    let head = server.head();
    assert_eq!(
        head,
        head_after_delete + kept.len() as u64,
        "exactly B's offline adds were logged"
    );
    a.pump_quiet(Duration::from_millis(400)).await;
    b.pump_quiet(Duration::from_millis(400)).await;
    assert_eq!(server.head(), head, "log is stable: no resurrection loop");
}
