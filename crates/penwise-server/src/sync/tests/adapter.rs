//! The real `Storage` behind a session: push → ack → broadcast echo, snapshot for a fresh client.

use std::path::PathBuf;

use penwise_proto::{ClientKind, ClientMsg, Scope, ServerMsg};

use super::fake::{device, notebook_op};
use super::{Rig, collect_catch_up, config, seqs};
use crate::storage::{Storage, StorageConfig};
use crate::sync::Hub;

fn data_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("penwise-sync-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).ok();
    dir
}

#[tokio::test]
async fn real_storage_push_ack_echo_and_fresh_client_snapshot() {
    let dir = data_dir("adapter");
    let storage = Storage::open(StorageConfig::new(dir.join("penwise.db"))).unwrap();
    let hub = Hub::new(storage.clone(), config());
    let mut a = Rig::on(hub.clone());
    let ServerMsg::Welcome {
        server, head_seq, ..
    } = a.connect(device(1), ClientKind::Tablet, 0).await
    else {
        panic!("Welcome")
    };
    assert_eq!((server, head_seq), (storage.server_id(), 0));
    collect_catch_up(&mut a).await;
    a.send(&ClientMsg::PushOps {
        ops: vec![notebook_op(1, 1), notebook_op(1, 2)],
    })
    .await;
    let ServerMsg::Ack {
        seqs: acked,
        rejected,
        ..
    } = a.next().await
    else {
        panic!("Ack")
    };
    assert_eq!((acked, rejected.len()), (vec![1, 2], 0));
    assert_eq!(seqs(&collect_catch_up(&mut a).await), vec![1, 2]);

    // Op of another device in the same frame: refused, the valid op still lands, the log stays gapless.
    a.send(&ClientMsg::PushOps {
        ops: vec![notebook_op(9, 3), notebook_op(1, 4)],
    })
    .await;
    let ServerMsg::Ack {
        seqs: acked,
        rejected,
        ..
    } = a.next().await
    else {
        panic!("Ack")
    };
    assert_eq!((acked, rejected.len()), (vec![], 1));
    // The frame was all-or-nothing; re-pushing the valid op alone lands at seq 3.
    a.send(&ClientMsg::PushOps {
        ops: vec![notebook_op(1, 4)],
    })
    .await;
    let ServerMsg::Ack { seqs: acked, .. } = a.next().await else {
        panic!("Ack")
    };
    assert_eq!(acked, vec![3]);

    let mut b = Rig::on(hub.clone());
    b.connect(device(2), ClientKind::Tablet, 0).await;
    assert_eq!(seqs(&collect_catch_up(&mut b).await), vec![1, 2, 3]);
    let mut c = Rig::on(hub.clone());
    c.connect(device(3), ClientKind::Browser, 0).await;
    assert_eq!(
        c.next().await,
        ServerMsg::SnapshotBegin {
            head_seq: 3,
            scope: Scope::Catalog
        }
    );
    let ServerMsg::SnapshotChunk(chunk) = c.next().await else {
        panic!("chunk")
    };
    assert_eq!(chunk.notebooks.len(), 3);
    assert_eq!(c.next().await, ServerMsg::SnapshotEnd { head_seq: 3 });
    std::fs::remove_dir_all(&dir).ok();
}
