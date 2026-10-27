//! H-1: two devices draw/erase offline, reconnect in both orders -> every replica and the server tables are identical.
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

use common::{TestServer, converge, line};
use penwise_proto::PageId;

/// Builds shared base state: one notebook, one page, three strokes, both devices synced.
async fn base(
    server: &TestServer,
) -> (
    penwise_testkit::TestDevice,
    penwise_testkit::TestDevice,
    PageId,
    Vec<penwise_core::item::ItemId>,
) {
    let mut a = server.tablet(1);
    let mut b = server.tablet(2);
    let nb = a.create_notebook("Main");
    let page = a.create_page(nb, "P1");
    let strokes: Vec<_> = (0..3_u16)
        .map(|i| a.draw(page, &line(20.0, 50.0 + f32::from(i) * 60.0, 40)))
        .collect();
    converge(server, &mut [&mut a, &mut b]).await;
    (a, b, page, strokes)
}

/// Offline edits per the design's flow E: A draws + precise-erases s0, B moves s0 and deletes s1.
fn offline_edits(
    a: &mut penwise_testkit::TestDevice,
    b: &mut penwise_testkit::TestDevice,
    page: PageId,
    s: &[penwise_core::item::ItemId],
) {
    a.draw(page, &line(300.0, 40.0, 30));
    let erased = a.erase(page, &[[60.0, 0.0], [60.0, 120.0]], 6.0);
    assert!(erased > 0, "the eraser must hit s0");
    b.move_item(page, s[0], 15.0, 15.0);
    b.delete_item(page, s[1]);
    b.draw(page, &line(500.0, 500.0, 25));
}

#[tokio::test]
async fn reconnect_a_then_b_converges() {
    let server = TestServer::start().await;
    let (mut a, mut b, page, s) = base(&server).await;
    a.offline();
    b.offline();
    offline_edits(&mut a, &mut b, page, &s);
    a.online().await.expect("a online");
    b.online().await.expect("b online");
    let fp = converge(&server, &mut [&mut a, &mut b]).await;
    assert!(
        fp.live_on(page) >= 3,
        "ink must survive: {}",
        fp.live_on(page)
    );
}

#[tokio::test]
async fn reconnect_b_then_a_converges_to_the_same_state() {
    let server_ab = TestServer::start().await;
    let (mut a, mut b, page, s) = base(&server_ab).await;
    a.offline();
    b.offline();
    offline_edits(&mut a, &mut b, page, &s);
    a.online().await.expect("a online");
    b.online().await.expect("b online");
    let fp_ab = converge(&server_ab, &mut [&mut a, &mut b]).await;

    // Same edits (same ids, stamps, op ids: devices are seeded) delivered in the other order on a fresh server.
    let server_ba = TestServer::start().await;
    let (mut a2, mut b2, page2, s2) = base(&server_ba).await;
    a2.offline();
    b2.offline();
    offline_edits(&mut a2, &mut b2, page2, &s2);
    b2.online().await.expect("b online");
    a2.online().await.expect("a online");
    let fp_ba = converge(&server_ba, &mut [&mut a2, &mut b2]).await;
    assert!(
        fp_ab.items.keys().eq(fp_ba.items.keys()),
        "same items regardless of arrival order"
    );
    assert_eq!(
        fp_ab.live_on(page),
        fp_ba.live_on(page2),
        "same live ink regardless of arrival order"
    );
}

#[tokio::test]
async fn duplicated_delivery_changes_nothing() {
    let server = TestServer::start().await;
    let (mut a, mut b, page, s) = base(&server).await;
    a.offline();
    offline_edits(&mut a, &mut b, page, &s);
    let before = a.outbox.pending_ops();
    a.online().await.expect("online");
    let fp = converge(&server, &mut [&mut a, &mut b]).await;
    // Re-push every op once more: all are duplicates, state and log stay identical.
    let head = server.head();
    for op in before {
        a.outbox.insert(&op).expect("insert");
    }
    a.settle(std::time::Duration::from_secs(10))
        .await
        .expect("settle");
    assert_eq!(
        server.head(),
        head,
        "duplicate pushes must not extend the log"
    );
    assert_eq!(server.fingerprint(), fp);
}
