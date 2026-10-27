//! A `Scope::Page` snapshot must not move the cursor past catalog ops the server never sent.
#![allow(
    clippy::unwrap_used,
    reason = "test harness: failures are reported by panicking"
)]

mod common;

use common::{device, notebook_create, notebook_id, page_create, page_id};
use penwise_proto::{HlcClock, Scope, SeqOp, ServerMsg, SnapshotChunk, encode_server};
use penwise_wasm::received::Received;
use penwise_wasm::viewer::Viewer;

fn send(viewer: &mut Viewer, msg: &ServerMsg) -> Received {
    viewer.receive(&encode_server(msg).unwrap()).unwrap()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn page_snapshot_end_keeps_the_cursor_so_catalog_ops_still_arrive() {
    let mut viewer = Viewer::new();
    let mut clock = HlcClock::new(device(1));
    let first = vec![
        notebook_create(&mut clock, 1, notebook_id(1), "Notes", 0),
        page_create(&mut clock, 2, page_id(2), notebook_id(1), "one", 0),
    ];
    let ops = first
        .into_iter()
        .zip(1..)
        .map(|(op, seq)| SeqOp { seq, op })
        .collect();
    send(
        &mut viewer,
        &ServerMsg::OpsSince {
            from_seq: 0,
            upto_seq: 2,
            ops,
            head_seq: 2,
            more: false,
        },
    );
    assert_eq!(viewer.head_seq(), 2);

    // Page 3 is created at seq 5 (not yet delivered) while the browser asks for page 2's snapshot at head 6.
    send(
        &mut viewer,
        &ServerMsg::SnapshotBegin {
            head_seq: 6,
            scope: Scope::Page(page_id(2)),
        },
    );
    send(
        &mut viewer,
        &ServerMsg::SnapshotChunk(SnapshotChunk::default()),
    );
    send(&mut viewer, &ServerMsg::SnapshotEnd { head_seq: 6 });
    assert_eq!(
        viewer.head_seq(),
        2,
        "page snapshot must not advance the cursor"
    );

    let late = SeqOp {
        seq: 5,
        op: page_create(&mut clock, 3, page_id(3), notebook_id(1), "late", 10),
    };
    let got = send(
        &mut viewer,
        &ServerMsg::OpsSince {
            from_seq: 2,
            upto_seq: 6,
            ops: vec![late],
            head_seq: 6,
            more: false,
        },
    );
    assert!(
        got.catalog_changed,
        "the late page create is applied, not skipped"
    );
    assert_eq!(viewer.pages(&notebook_id(1).to_string()).unwrap().len(), 2);
    assert_eq!(viewer.head_seq(), 6);

    // A catalog snapshot still advances the cursor.
    send(
        &mut viewer,
        &ServerMsg::SnapshotBegin {
            head_seq: 9,
            scope: Scope::Catalog,
        },
    );
    send(&mut viewer, &ServerMsg::SnapshotEnd { head_seq: 9 });
    assert_eq!(viewer.head_seq(), 9);
}
