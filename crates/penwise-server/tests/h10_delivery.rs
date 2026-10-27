//! H-10: subscribe-during-snapshot race, lagging consumers (tiny broadcast, tiny outbound queue), several concurrent
//! writers and three readers -> every reader sees every seq exactly once, in order.
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

use common::{Opts, TestServer, converge, line};
use penwise_testkit::TestDevice;

const WRITES: u16 = 25;

async fn write_many(mut d: TestDevice, page: penwise_proto::PageId, x: f32) -> TestDevice {
    for i in 0..WRITES {
        d.draw(page, &line(x, 10.0 + f32::from(i) * 4.0, 6));
        d.flush().await;
        assert!(
            d.pump_until(Duration::from_secs(10), |d| d.outbox.is_empty())
                .await,
            "ack"
        );
    }
    d
}

/// Contiguous, duplicate-free, ending at `head`; starts right after the snapshot (if any) or at 1.
fn assert_exactly_once(d: &TestDevice, head: u64, name: &str) {
    let delivered = &d.events.delivered;
    assert!(
        delivered.windows(2).all(|w| w[1] == w[0] + 1),
        "{name}: gaps, duplicates or reordering in {delivered:?}"
    );
    if let Some(&last) = delivered.last() {
        assert_eq!(last, head, "{name}: stopped short");
    }
    assert_eq!(d.replica.head_seq(), head, "{name}: cursor");
}

#[tokio::test]
async fn three_readers_see_each_seq_exactly_once_under_lag_and_races() {
    let server = TestServer::start_with(Opts {
        broadcast_capacity: Some(2),
        outbound_capacity: Some(2),
        ..Opts::default()
    })
    .await;
    let mut w1 = server.tablet(1);
    let mut w2 = server.tablet(2);
    let nb = w1.create_notebook("N");
    let page = w1.create_page(nb, "P");
    converge(&server, &mut [&mut w1, &mut w2]).await;

    let mut early = server.tablet(10);
    early.online().await.expect("early online");
    let mut slow = server.tablet(11);
    slow.online().await.expect("slow online");
    // `slow` stops reading now; its socket fills while the writers run.
    let t1 = tokio::spawn(write_many(w1, page, 10.0));
    let t2 = tokio::spawn(write_many(w2, page, 400.0));

    tokio::time::sleep(Duration::from_millis(150)).await;
    let mut joiner = server.tablet(12);
    joiner
        .online()
        .await
        .expect("joiner online during the write storm");

    let mut w1 = t1.await.expect("w1");
    let mut w2 = t2.await.expect("w2");
    let head = server.head();
    assert_eq!(head, 2 + 2 * u64::from(WRITES));

    let mut readers = [&mut early, &mut slow, &mut joiner];
    for r in &mut readers {
        assert!(
            r.pump_until(Duration::from_secs(20), |d| d.replica.head_seq() >= head)
                .await,
            "reader stuck"
        );
    }
    assert_exactly_once(&early, head, "early");
    assert_exactly_once(&slow, head, "slow");
    assert_exactly_once(&joiner, head, "joiner");
    assert_eq!(
        early.events.delivered.len() as u64,
        head,
        "early saw the whole log"
    );
    converge(
        &server,
        &mut [&mut w1, &mut w2, &mut early, &mut slow, &mut joiner],
    )
    .await;
}
