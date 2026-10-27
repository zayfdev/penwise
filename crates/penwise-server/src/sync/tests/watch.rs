//! Browser scope: catalog snapshot, per-page snapshots on `Watch`, filtered live ops, cursor across gaps.

use penwise_proto::{ClientKind, ClientMsg, Scope, ServerMsg};

use super::fake::{FakeStore, device, item_op, notebook_op, page_id, page_op};
use super::{Rig, seqs};

async fn browser(store: &FakeStore) -> Rig {
    let mut rig = Rig::new(store);
    rig.connect(device(1), ClientKind::Browser, 12_345).await;
    assert_eq!(
        rig.next().await,
        ServerMsg::SnapshotBegin {
            head_seq: store.head(),
            scope: Scope::Catalog
        }
    );
    rig.next().await;
    assert_eq!(
        rig.next().await,
        ServerMsg::SnapshotEnd {
            head_seq: store.head()
        }
    );
    rig
}

#[tokio::test]
async fn browser_ignores_its_cursor_and_always_starts_from_a_catalog_snapshot() {
    let store = FakeStore::new(16);
    store.commit(vec![notebook_op(2, 1)]);
    let _ = browser(&store).await;
    // The browser holds no cursor, so it never holds back the retention floor.
    assert_eq!(store.noted(), vec![(device(1), ClientKind::Browser, 1)]);
}

#[tokio::test]
async fn item_ops_arrive_only_for_the_watched_page_and_only_after_its_snapshot() {
    let store = FakeStore::new(16);
    let (p, q) = (page_id(1), page_id(2));
    store.commit(vec![notebook_op(2, 1), page_op(2, 2, p), page_op(2, 3, q)]);
    let mut rig = browser(&store).await;
    // Unwatched: catalog ops flow, item ops are filtered (cursor still advances).
    store.commit(vec![item_op(2, 10, p, "a")]);
    store.commit(vec![notebook_op(2, 11)]);
    assert_eq!(seqs(&[rig.next().await]), vec![5]);
    assert!(rig.quiet().await);
    // Watch p: its snapshot is taken at the current head.
    rig.send(&ClientMsg::Watch { page: Some(p) }).await;
    assert_eq!(
        rig.next().await,
        ServerMsg::SnapshotBegin {
            head_seq: 5,
            scope: Scope::Page(p)
        }
    );
    rig.next().await;
    assert_eq!(rig.next().await, ServerMsg::SnapshotEnd { head_seq: 5 });
    store.commit(vec![item_op(2, 12, p, "b"), item_op(2, 13, q, "c")]);
    let ServerMsg::OpsSince {
        from_seq,
        upto_seq,
        ops,
        ..
    } = rig.next().await
    else {
        panic!("OpsSince")
    };
    assert_eq!(
        (from_seq, upto_seq),
        (5, 7),
        "upto advances across the filtered op"
    );
    assert_eq!(ops.iter().map(|o| o.seq).collect::<Vec<_>>(), vec![6]);
    assert!(rig.quiet().await);
}

#[tokio::test]
async fn ops_already_inside_the_page_snapshot_are_not_replayed() {
    let store = FakeStore::new(16);
    let p = page_id(1);
    store.commit(vec![notebook_op(2, 1), page_op(2, 2, p)]);
    let mut rig = browser(&store).await;
    // seq 3 commits (and is broadcast) before the Watch is processed; the page snapshot includes it.
    store.commit(vec![item_op(2, 10, p, "a")]);
    rig.send(&ClientMsg::Watch { page: Some(p) }).await;
    // Everything committed so far is delivered (here: seq 3 is filtered out, only the cursor moves) before the
    // snapshot, so the snapshot never carries the client past an op it was not sent.
    assert!(matches!(
        rig.next().await,
        ServerMsg::OpsSince {
            from_seq: 2,
            upto_seq: 3,
            ..
        }
    ));
    assert!(matches!(
        rig.next().await,
        ServerMsg::SnapshotBegin {
            scope: Scope::Page(_),
            ..
        }
    ));
    rig.next().await;
    rig.next().await;
    assert!(
        rig.quiet().await,
        "the queued broadcast of seq 3 is inside the snapshot"
    );
}

#[tokio::test]
async fn browser_catch_up_after_lag_is_filtered_too() {
    let store = FakeStore::new(2);
    let p = page_id(1);
    store.commit(vec![notebook_op(2, 1), page_op(2, 2, p)]);
    let mut rig = browser(&store).await;
    rig.send(&ClientMsg::Watch { page: Some(p) }).await;
    rig.next().await;
    rig.next().await;
    rig.next().await;
    for n in 0..8 {
        store.commit(vec![item_op(2, 20 + n, page_id(2), "z")]);
        store.commit(vec![item_op(2, 40 + n, p, "w")]);
    }
    let mut got = Vec::new();
    while got.len() < 8 {
        match rig.next().await {
            ServerMsg::Resync { .. } => {}
            frame @ ServerMsg::OpsSince { .. } => got.extend(seqs(&[frame])),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(got, (0..8).map(|n| 4 + 2 * n).collect::<Vec<_>>());
}

#[tokio::test]
async fn a_catalog_op_committed_before_the_watch_is_delivered_before_the_page_snapshot() {
    let store = FakeStore::new(16);
    let (p, fresh) = (page_id(1), page_id(2));
    store.commit(vec![notebook_op(2, 1), page_op(2, 2, p)]);
    let mut rig = browser(&store).await;
    // Committed but not (yet) broadcast: the Watch finds it only in the log. The page snapshot is taken at
    // head 3, so a client that adopted that head would never see this page.
    store.commit_silently(vec![page_op(2, 3, fresh)]);
    rig.send(&ClientMsg::Watch { page: Some(p) }).await;
    let ServerMsg::OpsSince { ops, .. } = rig.next().await else {
        panic!("the new page op comes first")
    };
    assert_eq!(ops.iter().map(|o| o.seq).collect::<Vec<_>>(), vec![3]);
    assert!(matches!(
        rig.next().await,
        ServerMsg::SnapshotBegin {
            head_seq: 3,
            scope: Scope::Page(_)
        }
    ));
}
