//! Outbox lifecycle: insert, batch, ack, reject (dead letters), restart through the byte form.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "test harness: failures are reported by panicking"
)]

mod common;

use common::{Rng, device, item_id, item_op, op_id, page_id, text_item};
use penwise_core::edit::Edit;
use penwise_proto::message::{MAX_OPS_PER_FRAME, OpRejected};
use penwise_proto::outbox::{DeadOp, OUTBOX_VERSION};
use penwise_proto::{AnyOp, ClientMsg, HlcClock, OpRejection, Outbox, ProtoError, encode_client};

fn op(clock: &mut HlcClock, n: u64) -> AnyOp {
    item_op(
        clock,
        1_000 + n,
        n,
        page_id(1),
        Edit::Add(text_item(item_id(n), 0, "x")),
    )
}

fn big_op(clock: &mut HlcClock, n: u64) -> AnyOp {
    let content = "x".repeat(900_000);
    item_op(
        clock,
        1_000 + n,
        n,
        page_id(1),
        Edit::Add(text_item(item_id(n), 0, &content)),
    )
}

fn ids(ops: &[AnyOp]) -> Vec<penwise_proto::OpId> {
    ops.iter().map(AnyOp::op_id).collect()
}

fn filled(count: u64) -> Outbox {
    let mut clock = HlcClock::new(device(1));
    let mut outbox = Outbox::new();
    for n in 1..=count {
        assert!(outbox.insert(&op(&mut clock, n)).unwrap());
    }
    outbox
}

#[test]
fn batches_follow_creation_order_and_ops_stay_pending() {
    let mut outbox = filled(5);
    let first = outbox.next_batch();
    assert_eq!(ids(&first), (1..=5).map(op_id).collect::<Vec<_>>());
    assert_eq!(outbox.len(), 5, "pushing does not remove: only an ack does");
    assert_eq!(
        ids(&outbox.next_batch()),
        ids(&first),
        "re-push after a reconnect is identical"
    );
    assert_eq!(ids(&outbox.pending_ops()), ids(&first));
}

#[test]
fn duplicate_op_ids_are_not_queued_twice() {
    let mut clock = HlcClock::new(device(1));
    let mut outbox = Outbox::new();
    let a = op(&mut clock, 1);
    assert!(outbox.insert(&a).unwrap());
    assert!(!outbox.insert(&a).unwrap());
    assert_eq!(outbox.len(), 1);
}

#[test]
fn a_batch_never_exceeds_the_op_count_or_frame_budget() {
    let mut outbox = filled(600);
    let batch = outbox.next_batch();
    assert_eq!(batch.len(), MAX_OPS_PER_FRAME);
    assert!(encode_client(&ClientMsg::PushOps { ops: batch }).is_ok());
    let acked: Vec<_> = (1..=512).map(op_id).collect();
    assert_eq!(outbox.ack(&acked), 512);
    assert_eq!(
        ids(&outbox.next_batch()),
        (513..=600).map(op_id).collect::<Vec<_>>()
    );

    let mut clock = HlcClock::new(device(1));
    let mut heavy = Outbox::new();
    for n in 1..=6 {
        heavy.insert(&big_op(&mut clock, n)).unwrap();
    }
    let batch = heavy.next_batch();
    assert!(
        (1..6).contains(&batch.len()),
        "byte budget splits the queue, got {}",
        batch.len()
    );
    assert!(
        encode_client(&ClientMsg::PushOps { ops: batch }).is_ok(),
        "the batch fits one frame"
    );
}

#[test]
fn ack_removes_only_the_listed_ops_and_is_idempotent() {
    let mut outbox = filled(5);
    assert_eq!(outbox.ack(&[op_id(4), op_id(2)]), 2);
    assert_eq!(
        ids(&outbox.pending_ops()),
        vec![op_id(1), op_id(3), op_id(5)]
    );
    assert_eq!(
        outbox.ack(&[op_id(4), op_id(99)]),
        0,
        "a repeated or unknown ack changes nothing"
    );
    assert!(outbox.contains(op_id(3)) && !outbox.contains(op_id(2)));
    assert_eq!(outbox.ack(&[op_id(1), op_id(3), op_id(5)]), 3);
    assert!(outbox.is_empty());
    // An acked id may be queued again (new session reusing nothing): it is no longer tracked.
    let mut clock = HlcClock::new(device(1));
    assert!(outbox.insert(&op(&mut clock, 1)).unwrap());
}

#[test]
fn rejected_ops_become_dead_letters_with_their_reason_and_are_never_dropped() {
    let mut outbox = filled(4);
    let moved = outbox.reject(&[
        OpRejected {
            op_id: op_id(2),
            reason: OpRejection::UnknownItem,
        },
        OpRejected {
            op_id: op_id(3),
            reason: OpRejection::Invalid,
        },
        OpRejected {
            op_id: op_id(50),
            reason: OpRejection::Duplicate,
        },
    ]);
    assert_eq!(moved, 2);
    assert_eq!(ids(&outbox.pending_ops()), vec![op_id(1), op_id(4)]);
    let dead: Vec<_> = outbox.dead().iter().map(|d| (d.op_id, d.reason)).collect();
    assert_eq!(
        dead,
        vec![
            (op_id(2), OpRejection::UnknownItem),
            (op_id(3), OpRejection::Invalid)
        ]
    );
    // A late ack of a dead op does not resurrect or drop it.
    assert_eq!(outbox.ack(&[op_id(2)]), 0);
    assert_eq!(outbox.dead().len(), 2);
    // The dead op's bytes still decode (the UI can show/export it).
    let letter = &outbox.dead()[0];
    assert_eq!(
        penwise_proto::decode_op(&letter.bytes).unwrap().op_id(),
        op_id(2)
    );
    // The id stays reserved while dead, and frees up only when the user removes the letter.
    let mut clock = HlcClock::new(device(1));
    assert!(!outbox.insert(&op(&mut clock, 2)).unwrap());
    assert_eq!(
        outbox.remove_dead(op_id(2)).unwrap().reason,
        OpRejection::UnknownItem
    );
    assert_eq!(outbox.remove_dead(op_id(2)), None);
    assert_eq!(outbox.dead().len(), 1);
}

#[test]
fn restart_restores_pending_order_dead_letters_and_dedupe() {
    let mut outbox = filled(6);
    outbox.ack(&[op_id(1), op_id(2)]);
    outbox.reject(&[OpRejected {
        op_id: op_id(3),
        reason: OpRejection::WrongPage,
    }]);
    let bytes = outbox.to_bytes().unwrap();
    assert_eq!(bytes[0], OUTBOX_VERSION);
    let mut restored = Outbox::from_bytes(&bytes).unwrap();
    assert_eq!(restored, outbox);
    // Pending ops are pushed again, in order, exactly as before the crash.
    assert_eq!(
        ids(&restored.next_batch()),
        vec![op_id(4), op_id(5), op_id(6)]
    );
    // The restored queue still dedupes pending and dead ids.
    let mut clock = HlcClock::new(device(1));
    assert!(!restored.insert(&op(&mut clock, 5)).unwrap());
    assert!(!restored.insert(&op(&mut clock, 3)).unwrap());
    assert!(restored.insert(&op(&mut clock, 7)).unwrap());
    // Row-wise persistence reaches the same state.
    let rows: Vec<(penwise_proto::OpId, Vec<u8>)> = restored
        .pending_entries()
        .map(|(id, b)| (id, b.to_vec()))
        .collect();
    let again = Outbox::from_parts(rows, restored.dead().to_vec()).unwrap();
    assert_eq!(again, restored);
}

#[test]
fn restart_then_ack_converges_with_an_uninterrupted_run() {
    let mut live = filled(10);
    let mut crashed = Outbox::from_bytes(&live.to_bytes().unwrap()).unwrap();
    for outbox in [&mut live, &mut crashed] {
        let _ = outbox.next_batch();
        outbox.ack(&(1..=7).map(op_id).collect::<Vec<_>>());
    }
    assert_eq!(live, crashed);
    assert_eq!(live.len(), 3);
}

#[test]
fn bad_stored_forms_are_rejected() {
    assert_eq!(Outbox::from_bytes(&[]), Err(ProtoError::Empty));
    assert_eq!(
        Outbox::from_bytes(&[9, 0, 0]),
        Err(ProtoError::UnsupportedVersion(9))
    );
    let mut trailing = Outbox::new().to_bytes().unwrap();
    trailing.push(0);
    assert_eq!(
        Outbox::from_bytes(&trailing),
        Err(ProtoError::TrailingBytes { count: 1 })
    );
    assert!(matches!(
        Outbox::from_parts(vec![(op_id(1), vec![1]), (op_id(1), vec![1])], Vec::new()),
        Err(ProtoError::Invalid(_))
    ));
    let dead = DeadOp {
        op_id: op_id(1),
        bytes: vec![1],
        reason: OpRejection::Invalid,
    };
    assert!(matches!(
        Outbox::from_parts(vec![(op_id(1), vec![1])], vec![dead]),
        Err(ProtoError::Invalid(_))
    ));
    // Totality on garbage.
    let mut rng = Rng(5);
    for _ in 0..5_000 {
        let len = rng.below(80);
        let mut bytes: Vec<u8> = (0..len).map(|_| rng.next_u64().to_le_bytes()[0]).collect();
        if !bytes.is_empty() {
            bytes[0] = OUTBOX_VERSION;
        }
        let _ = Outbox::from_bytes(&bytes);
    }
}

#[test]
fn an_undecodable_stored_op_is_quarantined_instead_of_blocking_the_queue() {
    let mut clock = HlcClock::new(device(1));
    let good = op(&mut clock, 2);
    let good_bytes = penwise_proto::encode_op(&good).unwrap();
    let mut outbox = Outbox::from_parts(
        vec![(op_id(1), vec![1, 0xFF, 0xFF]), (op_id(2), good_bytes)],
        Vec::new(),
    )
    .unwrap();
    assert_eq!(ids(&outbox.next_batch()), vec![op_id(2)]);
    assert_eq!(outbox.dead().len(), 1);
    assert_eq!(outbox.dead()[0].op_id, op_id(1));
    assert_eq!(outbox.dead()[0].reason, OpRejection::Invalid);
    assert_eq!(outbox.len(), 1);
}
