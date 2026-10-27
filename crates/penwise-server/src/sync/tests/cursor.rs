use penwise_proto::SeqOp;

use super::fake::{item_op, notebook_op, page_id};
use crate::sync::OpFilter;
use crate::sync::cursor::{Delivery, classify, visible};
use crate::sync::limiter::{HandshakeLimiter, MAX_FAILURES, WINDOW_MS};

fn batch(from: u64, to: u64) -> Vec<SeqOp> {
    (from..=to)
        .map(|seq| SeqOp {
            seq,
            op: notebook_op(1, seq),
        })
        .collect()
}

#[test]
fn contiguous_batch_is_delivered_whole() {
    assert_eq!(classify(4, &batch(5, 7)), Delivery::Deliver { start: 0 });
}

#[test]
fn batch_at_or_below_cursor_is_skipped() {
    assert_eq!(classify(7, &batch(5, 7)), Delivery::Skip);
    assert_eq!(classify(9, &batch(5, 7)), Delivery::Skip);
    assert_eq!(classify(0, &[]), Delivery::Skip);
}

#[test]
fn overlapping_batch_delivers_only_the_new_tail() {
    assert_eq!(classify(5, &batch(5, 8)), Delivery::Deliver { start: 1 });
    assert_eq!(classify(7, &batch(5, 8)), Delivery::Deliver { start: 3 });
}

#[test]
fn batch_beyond_cursor_plus_one_is_a_gap() {
    assert_eq!(classify(3, &batch(5, 7)), Delivery::Gap);
}

#[test]
fn browser_filter_keeps_catalog_and_watched_page_after_snapshot() {
    let (p, q) = (page_id(1), page_id(2));
    let filter = OpFilter::Catalog {
        page: Some(p),
        page_from_seq: 10,
    };
    let at = |seq, op| SeqOp { seq, op };
    assert!(visible(filter, &at(5, notebook_op(1, 1))));
    assert!(visible(filter, &at(11, item_op(1, 2, p, "x"))));
    assert!(
        !visible(filter, &at(10, item_op(1, 3, p, "x"))),
        "inside the snapshot"
    );
    assert!(
        !visible(filter, &at(11, item_op(1, 4, q, "x"))),
        "other page"
    );
    let none = OpFilter::Catalog {
        page: None,
        page_from_seq: 0,
    };
    assert!(!visible(none, &at(11, item_op(1, 5, p, "x"))));
    assert!(visible(OpFilter::All, &at(1, item_op(1, 6, q, "x"))));
}

#[test]
fn limiter_refuses_after_too_many_failures_and_recovers() {
    let mut limiter = HandshakeLimiter::default();
    let peer = None;
    for _ in 0..MAX_FAILURES {
        assert!(limiter.allowed(peer, 1_000));
        limiter.failed(peer, 1_000);
    }
    assert!(!limiter.allowed(peer, 2_000));
    assert!(limiter.allowed(
        Some("10.0.0.2".parse().unwrap_or_else(|_| unreachable!())),
        2_000
    ));
    assert!(limiter.allowed(peer, 1_000 + WINDOW_MS));
    limiter.failed(peer, 1_000 + WINDOW_MS);
    limiter.succeeded(peer);
    assert!(limiter.allowed(peer, 1_000 + WINDOW_MS));
}
