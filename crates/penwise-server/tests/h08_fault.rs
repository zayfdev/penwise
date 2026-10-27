//! H-8: the writer fails in the middle of a `PushOps` transaction (fault injection) -> the frame is all-or-nothing,
//! the client's re-push dedupes, and `seq` stays gap-free.
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
use penwise_server::storage::FaultPlan;
use penwise_testkit::check;

#[tokio::test]
async fn a_failed_frame_logs_nothing_and_the_repush_lands_once_without_gaps() {
    // The setup frame (notebook + page = 2 ops) stays below the threshold; the stroke frame trips it.
    let server = TestServer::start_with(Opts {
        fault: Some(FaultPlan::new(3)),
        ..Opts::default()
    })
    .await;
    let mut a = server.tablet(1);
    let mut b = server.tablet(2);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "P");
    converge(&server, &mut [&mut a, &mut b]).await;
    let head = server.head();
    let fp = server.fingerprint();

    for i in 0..6_u16 {
        a.draw(page, &line(10.0, 20.0 + f32::from(i) * 20.0, 15));
    }
    assert_eq!(a.outbox.len(), 6);
    a.flush().await;
    a.pump_quiet(Duration::from_millis(500)).await;

    assert_eq!(
        server.head(),
        head,
        "the failed frame logged nothing (all-or-nothing)"
    );
    assert_eq!(server.fingerprint(), fp, "tables untouched");
    assert_eq!(
        a.outbox.len(),
        6,
        "the client keeps every op until it is acked"
    );
    assert!(
        a.outbox.dead().is_empty(),
        "a storage fault is not a rejection"
    );
    b.pump_quiet(Duration::from_millis(200)).await;
    assert_eq!(
        b.replica.head_seq(),
        head,
        "nothing was broadcast for the failed frame"
    );

    // Re-push (fault disarmed after one shot): lands once.
    a.online().await.expect("reconnect and re-push");
    assert_eq!(server.head(), head + 6);
    converge(&server, &mut [&mut a, &mut b]).await;

    let seqs = check::db_seqs(&server.db_path()).expect("seqs");
    let want: Vec<u64> = (1..=seqs.len() as u64).collect();
    assert_eq!(seqs, want, "seq has no gaps after a rolled-back frame");
}

#[tokio::test]
async fn restarting_the_server_between_push_and_ack_dedupes_the_repush() {
    let mut server = TestServer::start().await;
    let mut a = server.tablet(1);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "P");
    converge(&server, &mut [&mut a]).await;
    for i in 0..4_u16 {
        a.draw(page, &line(10.0, 20.0 + f32::from(i) * 20.0, 15));
    }
    a.online().await.expect("push lands");
    let head = server.head();

    // The ack may have been lost: the device (after a crash that kept its outbox) pushes the same ops again.
    let ops = a.outbox.pending_ops();
    assert!(ops.is_empty());
    server.restart().await;
    a.crash_restart(true);
    let page_ops = server
        .storage
        .ops_since(0, penwise_server::storage::OpFilter::All, 10_000)
        .await
        .expect("log");
    for op in page_ops.ops.iter().rev().take(4) {
        a.outbox.insert(&op.op).expect("insert");
    }
    a.online().await.expect("reconnect");
    assert_eq!(server.head(), head, "re-pushed ops dedupe by op id");
    let seqs = check::db_seqs(&server.db_path()).expect("seqs");
    assert_eq!(seqs, (1..=head).collect::<Vec<_>>());
}
