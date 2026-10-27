//! Catch-up, snapshots, pushes, the subscribe-during-snapshot race, lag recovery.

use penwise_proto::message::MAX_OPS_PER_FRAME;
use penwise_proto::{ClientKind, ClientMsg, OpRejected, OpRejection, Scope, ServerMsg};

use super::fake::{FakeStore, device, item_op, notebook_op, page_id};
use super::{Rig, collect_catch_up, config, seqs};
use crate::sync::Hub;

fn notebooks(store: &FakeStore, count: u64) {
    store.commit((1..=count).map(|n| notebook_op(1, n)).collect());
}

#[tokio::test]
async fn catch_up_pages_chain_and_cover_every_op_once() {
    let store = FakeStore::new(16);
    notebooks(&store, 1_300);
    let mut rig = Rig::new(&store);
    assert!(matches!(
        rig.connect(device(2), ClientKind::Tablet, 100).await,
        ServerMsg::Welcome {
            head_seq: 1_300,
            ..
        }
    ));
    let frames = collect_catch_up(&mut rig).await;
    assert_eq!(seqs(&frames), (101..=1_300).collect::<Vec<_>>());
    let mut cursor = 100;
    for frame in &frames {
        let ServerMsg::OpsSince {
            from_seq,
            upto_seq,
            ops,
            more,
            head_seq,
        } = frame
        else {
            panic!("only OpsSince expected");
        };
        assert_eq!(*from_seq, cursor);
        assert!(ops.len() <= MAX_OPS_PER_FRAME);
        assert_eq!(*upto_seq, ops.last().map_or(cursor, |o| o.seq));
        assert_eq!(*head_seq, 1_300);
        assert_eq!(*more, *upto_seq < 1_300);
        cursor = *upto_seq;
    }
    assert!(store.snapshot_calls().is_empty());
}

#[tokio::test]
async fn caught_up_client_gets_one_empty_final_batch() {
    let store = FakeStore::new(16);
    notebooks(&store, 3);
    let mut rig = Rig::new(&store);
    rig.connect(device(2), ClientKind::Tablet, 3).await;
    assert_eq!(
        rig.next().await,
        ServerMsg::OpsSince {
            from_seq: 3,
            upto_seq: 3,
            ops: Vec::new(),
            head_seq: 3,
            more: false
        }
    );
}

#[tokio::test]
async fn behind_floor_or_ahead_of_head_gets_a_snapshot_not_a_diff() {
    for (last_seen, floor) in [(2, 5), (50, 0)] {
        let store = FakeStore::new(16);
        notebooks(&store, 10);
        store.set_floor(floor);
        let mut rig = Rig::new(&store);
        rig.connect(device(2), ClientKind::Tablet, last_seen).await;
        assert_eq!(
            rig.next().await,
            ServerMsg::SnapshotBegin {
                head_seq: 10,
                scope: Scope::All
            }
        );
        assert!(matches!(rig.next().await, ServerMsg::SnapshotChunk(_)));
        assert_eq!(rig.next().await, ServerMsg::SnapshotEnd { head_seq: 10 });
        assert_eq!(store.snapshot_calls(), vec![Scope::All]);
    }
}

#[tokio::test]
async fn log_compacted_during_catch_up_falls_back_to_a_snapshot() {
    let store = FakeStore::new(16);
    notebooks(&store, 10);
    store.fail_next_ops_since_below_floor();
    let mut rig = Rig::new(&store);
    rig.connect(device(2), ClientKind::Tablet, 4).await;
    assert_eq!(
        rig.next().await,
        ServerMsg::SnapshotBegin {
            head_seq: 10,
            scope: Scope::All
        }
    );
}

#[tokio::test]
async fn push_acks_in_order_and_echoes_the_batch_to_the_author() {
    let store = FakeStore::new(16);
    let mut rig = Rig::new(&store);
    rig.connect(device(1), ClientKind::Tablet, 0).await;
    collect_catch_up(&mut rig).await;
    rig.send(&ClientMsg::PushOps {
        ops: vec![notebook_op(1, 1), notebook_op(1, 2)],
    })
    .await;
    let ServerMsg::Ack {
        op_ids,
        seqs,
        rejected,
    } = rig.next().await
    else {
        panic!("Ack")
    };
    assert_eq!(op_ids, vec![super::fake::op_id(1), super::fake::op_id(2)]);
    assert_eq!(seqs, vec![1, 2]);
    assert!(rejected.is_empty());
    let echo = collect_catch_up(&mut rig).await;
    assert_eq!(self::seqs(&echo), vec![1, 2]);
    // Idempotent re-push: same seqs, nothing new broadcast.
    rig.send(&ClientMsg::PushOps {
        ops: vec![notebook_op(1, 1)],
    })
    .await;
    let ServerMsg::Ack { seqs, .. } = rig.next().await else {
        panic!("Ack")
    };
    assert_eq!(seqs, vec![1]);
    assert!(rig.quiet().await);
}

#[tokio::test]
async fn frame_with_a_refused_op_logs_nothing_and_the_valid_op_can_be_repushed() {
    let store = FakeStore::new(16);
    let mut rig = Rig::new(&store);
    rig.connect(device(1), ClientKind::Tablet, 0).await;
    collect_catch_up(&mut rig).await;
    // Op 2 is authored by device 9, not the session device.
    rig.send(&ClientMsg::PushOps {
        ops: vec![notebook_op(1, 1), notebook_op(9, 2)],
    })
    .await;
    let ServerMsg::Ack {
        op_ids,
        seqs,
        rejected,
    } = rig.next().await
    else {
        panic!("Ack")
    };
    assert!(op_ids.is_empty() && seqs.is_empty());
    assert_eq!(
        rejected,
        vec![OpRejected {
            op_id: super::fake::op_id(2),
            reason: OpRejection::NotOwnDevice
        }]
    );
    assert_eq!(store.head(), 0, "the valid op must not be applied alone");
    assert!(rig.quiet().await);
    // Outbox path: ack nothing, dead-letter the refused op, re-push what is still pending.
    let mut outbox = penwise_proto::Outbox::new();
    for op in [notebook_op(1, 1), notebook_op(9, 2)] {
        outbox.insert(&op).unwrap();
    }
    outbox.ack(&op_ids);
    outbox.reject(&rejected);
    let retry = outbox.next_batch();
    assert_eq!(retry, vec![notebook_op(1, 1)]);
    rig.send(&ClientMsg::PushOps { ops: retry }).await;
    let ServerMsg::Ack {
        op_ids,
        seqs,
        rejected,
    } = rig.next().await
    else {
        panic!("Ack")
    };
    assert_eq!(
        (op_ids, seqs, rejected.len()),
        (vec![super::fake::op_id(1)], vec![1], 0)
    );
    outbox.ack(&[super::fake::op_id(1)]);
    assert!(outbox.is_empty() && outbox.dead().len() == 1);
}

#[tokio::test]
async fn all_refused_frame_logs_nothing() {
    let store = FakeStore::new(16);
    let mut rig = Rig::new(&store);
    rig.connect(device(1), ClientKind::Tablet, 0).await;
    collect_catch_up(&mut rig).await;
    rig.send(&ClientMsg::PushOps {
        ops: vec![notebook_op(9, 1)],
    })
    .await;
    let ServerMsg::Ack {
        op_ids,
        seqs,
        rejected,
    } = rig.next().await
    else {
        panic!("Ack")
    };
    assert!(op_ids.is_empty() && seqs.is_empty());
    assert_eq!(rejected.len(), 1);
    assert_eq!(store.head(), 0);
}

#[tokio::test]
async fn ops_committed_by_others_reach_a_tablet_exactly_once() {
    let store = FakeStore::new(16);
    let mut rig = Rig::new(&store);
    rig.connect(device(1), ClientKind::Tablet, 0).await;
    collect_catch_up(&mut rig).await;
    store.commit(vec![notebook_op(2, 1)]);
    store.commit(vec![notebook_op(2, 2), notebook_op(2, 3)]);
    let first = rig.next().await;
    let second = rig.next().await;
    assert_eq!(seqs(&[first, second]), vec![1, 2, 3]);
    assert!(rig.quiet().await);
}

#[tokio::test]
async fn commit_during_the_snapshot_read_is_delivered_once_after_it() {
    let store = FakeStore::new(16);
    notebooks(&store, 5);
    // After the snapshot's head is read, seq 6 commits and is broadcast.
    store.on_snapshot(|s| {
        s.commit(vec![notebook_op(2, 6)]);
    });
    let mut rig = Rig::new(&store);
    rig.connect(device(1), ClientKind::Tablet, 99).await;
    assert_eq!(
        rig.next().await,
        ServerMsg::SnapshotBegin {
            head_seq: 5,
            scope: Scope::All
        }
    );
    rig.next().await;
    assert_eq!(rig.next().await, ServerMsg::SnapshotEnd { head_seq: 5 });
    let live = rig.next().await;
    assert_eq!(seqs(&[live]), vec![6]);
    assert!(rig.quiet().await);
}

#[tokio::test]
async fn commit_before_the_head_read_but_broadcast_after_is_not_duplicated() {
    let store = FakeStore::new(16);
    notebooks(&store, 5);
    let pending = std::sync::Arc::new(std::sync::Mutex::new(None));
    let slot = std::sync::Arc::clone(&pending);
    // Seq 6 is already in the snapshot (committed before the head read); its broadcast lands late.
    store.on_snapshot(move |s| {
        let batch = s.commit_silently(vec![notebook_op(2, 6)]);
        slot.lock().map(|mut g| *g = Some(batch)).ok();
    });
    let mut rig = Rig::new(&store);
    rig.connect(device(1), ClientKind::Tablet, 99).await;
    let ServerMsg::SnapshotBegin { head_seq, .. } = rig.next().await else {
        panic!("snapshot")
    };
    // The fake read head before the hook committed 6: the snapshot says 5; the late broadcast of 6 is new.
    assert_eq!(head_seq, 5);
    rig.next().await;
    rig.next().await;
    let batch = pending
        .lock()
        .ok()
        .and_then(|mut g| g.take())
        .unwrap_or_else(|| Vec::new().into());
    store.publish(batch);
    assert_eq!(seqs(&[rig.next().await]), vec![6]);
    // A re-broadcast of the same batch changes nothing.
    store.publish(
        store
            .log()
            .into_iter()
            .filter(|e| e.seq == 6)
            .collect::<Vec<_>>()
            .into(),
    );
    assert!(rig.quiet().await);
}

#[tokio::test]
async fn lagging_session_resyncs_from_its_cursor_without_gap_or_duplicate() {
    let store = FakeStore::new(2);
    let mut cfg = config();
    cfg.outbound_capacity = 1;
    let mut rig = Rig::on(Hub::new(store.clone(), cfg));
    rig.connect(device(1), ClientKind::Tablet, 0).await;
    collect_catch_up(&mut rig).await;
    // Do not read: the session blocks on its tiny queue while 12 batches overflow the 2-slot broadcast.
    for n in 1..=12 {
        store.commit(vec![notebook_op(2, n)]);
        tokio::task::yield_now().await;
    }
    let mut delivered = Vec::new();
    let mut resyncs = 0;
    while delivered.len() < 12 {
        match rig.next().await {
            ServerMsg::Resync { .. } => resyncs += 1,
            frame @ ServerMsg::OpsSince { .. } => delivered.extend(seqs(&[frame])),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(delivered, (1..=12).collect::<Vec<_>>());
    assert!(resyncs >= 1, "the overflow must have been noticed");
    assert!(rig.quiet().await);
}

#[tokio::test]
async fn client_resync_inside_the_log_replays_from_there_below_floor_snapshots() {
    let store = FakeStore::new(16);
    notebooks(&store, 8);
    let mut rig = Rig::new(&store);
    rig.connect(device(1), ClientKind::Tablet, 8).await;
    collect_catch_up(&mut rig).await;
    rig.send(&ClientMsg::Resync { from_seq: 5 }).await;
    assert_eq!(rig.next().await, ServerMsg::Resync { from_seq: 5 });
    assert_eq!(seqs(&collect_catch_up(&mut rig).await), vec![6, 7, 8]);
    store.set_floor(7);
    rig.send(&ClientMsg::Resync { from_seq: 5 }).await;
    assert_eq!(rig.next().await, ServerMsg::Resync { from_seq: 5 });
    assert_eq!(
        rig.next().await,
        ServerMsg::SnapshotBegin {
            head_seq: 8,
            scope: Scope::All
        }
    );
}

#[tokio::test]
async fn oversized_batches_split_into_contiguous_frames() {
    let store = FakeStore::new(16);
    let page = page_id(1);
    let big = "x".repeat(900_000);
    store.commit((1..=6).map(|n| item_op(1, n, page, &big)).collect());
    let mut rig = Rig::new(&store);
    rig.connect(device(2), ClientKind::Tablet, 0).await;
    let frames = collect_catch_up(&mut rig).await;
    assert!(frames.len() >= 2, "6 × 900 KB cannot fit one 4 MiB frame");
    assert_eq!(seqs(&frames), (1..=6).collect::<Vec<_>>());
    let mut cursor = 0;
    for frame in &frames {
        let ServerMsg::OpsSince {
            from_seq,
            upto_seq,
            ops,
            ..
        } = frame
        else {
            panic!("OpsSince")
        };
        assert_eq!(
            (*from_seq, *upto_seq),
            (cursor, ops.last().map_or(cursor, |o| o.seq))
        );
        cursor = *upto_seq;
    }
}
