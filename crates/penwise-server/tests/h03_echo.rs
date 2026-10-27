//! H-3: the device's own `OpsSince` echo of a precise-erase gesture leaves the scene unchanged (no duplicate fragments).
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
use penwise_proto::AnyOp;
use penwise_testkit::check::fingerprint;

#[tokio::test]
async fn echo_of_own_erase_gesture_is_a_no_op() {
    let server = TestServer::start().await;
    let mut a = server.tablet(1);
    let mut b = server.tablet(2);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "P");
    for i in 0..3_u16 {
        a.draw(page, &line(20.0, 60.0 + f32::from(i) * 50.0, 50));
    }
    converge(&server, &mut [&mut a, &mut b]).await;
    let live_before = a.live_items(page).len();

    let edits = a.erase(page, &[[80.0, 20.0], [80.0, 220.0]], 8.0);
    assert!(edits > 1, "the gesture must split strokes (adds + deletes)");
    let fp = converge(&server, &mut [&mut a, &mut b]).await;
    let after_sync = a.live_items(page);
    assert!(
        after_sync.len() > live_before,
        "splitting produces more pieces than before"
    );

    // Re-deliver the complete log to A out of cursor order (the echo case): the scene must not change.
    let mut seen = std::collections::BTreeSet::new();
    let page_ops = server
        .storage
        .ops_since(0, penwise_server::storage::OpFilter::All, 10_000)
        .await
        .expect("log");
    for seq_op in &page_ops.ops {
        a.replica
            .apply(&seq_op.op)
            .expect("re-applying a logged op is a no-op, not an error");
        if let AnyOp::Item(op) = &seq_op.op {
            seen.insert(op.op_id);
        }
    }
    assert!(!seen.is_empty());
    assert_eq!(fingerprint(&a.replica), fp, "scene unchanged after echo");
    assert_eq!(a.live_items(page), after_sync, "no duplicate fragments");
    assert_eq!(
        b.live_items(page).len(),
        after_sync.len(),
        "other device holds the same pieces"
    );
}
