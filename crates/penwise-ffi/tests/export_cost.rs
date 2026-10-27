//! State export cost: the part that must run under the host's lock (`snapshot_state`) against the whole
//! export (`export_state`) on a library of 3000 strokes (all acknowledged: an empty outbox, the usual case).
//! Numbers go to stderr (`--nocapture`); the ratio is asserted, absolute times are only reported.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use std::time::Instant;

use penwise_core::edit::Edit;
use penwise_core::item::{ItemId, Layer};
use penwise_ffi::sync_core::{NewPage, SyncCore};
use penwise_proto::{Background, DeviceId, PageKind, SeqOp, ServerMsg, encode_server};
use penwise_testkit::stroke_item;

#[test]
fn snapshot_state_is_much_cheaper_than_the_export_and_encodes_the_same_bytes() {
    let mut author = SyncCore::new(DeviceId::from_parts(1_000, 1), 1);
    let core = &mut author;
    let notebook = core.create_notebook(None, "nb", 0, 5_000).unwrap();
    let page = core
        .create_page(
            None,
            NewPage {
                notebook,
                title: "p".to_owned(),
                position: 0,
                kind: PageKind::Infinite,
                background: Background::Blank,
            },
            5_000,
        )
        .unwrap();
    for n in 0..3_000_u32 {
        let line: Vec<[f32; 2]> = (0..120_u16)
            .map(|i| {
                [
                    f32::from(i) * 2.0,
                    (f32::from(i) * 0.3).sin() * 20.0 + n as f32,
                ]
            })
            .collect();
        let mut id = [0_u8; 16];
        id[..4].copy_from_slice(&n.to_be_bytes());
        let item = stroke_item(
            ItemId(id),
            i64::from(n + 1) * 65_536,
            Layer::Ink,
            u64::from(n),
            &line,
        );
        core.commit_edits(page, &[Edit::Add(item)], 5_000 + u64::from(n))
            .unwrap();
    }

    // A second device that only received the library: nothing pending, like a tablet after catch-up.
    let ops = author.outbox().pending_ops();
    let mut core = SyncCore::new(DeviceId::from_parts(1_000, 2), 2);
    for (from, batch) in (0_u64..).step_by(500).zip(ops.chunks(500)) {
        let seq_ops: Vec<SeqOp> = batch
            .iter()
            .zip(from + 1..)
            .map(|(op, seq)| SeqOp {
                seq,
                op: op.clone(),
            })
            .collect();
        let upto = from + seq_ops.len() as u64;
        let frame = encode_server(&ServerMsg::OpsSince {
            from_seq: from,
            upto_seq: upto,
            ops: seq_ops,
            head_seq: upto,
            more: false,
        })
        .unwrap();
        assert_eq!(core.receive(&frame, 6_000).unwrap().apply_errors, 0);
    }
    assert_eq!(core.item_ids(page).len(), 3_000);
    assert!(core.outbox().is_empty());

    let t = Instant::now();
    let snapshot = core.snapshot_state().unwrap();
    let under_lock = t.elapsed();
    let t = Instant::now();
    let off_lock = snapshot.encode().unwrap();
    let encode = t.elapsed();
    let t = Instant::now();
    let blocking = core.export_state().unwrap();
    let whole = t.elapsed();
    eprintln!(
        "3000 strokes x 120 samples: snapshot under lock {under_lock:?}, encode off lock {encode:?}, \\
         blocking export {whole:?}, {} bytes",
        blocking.len()
    );
    assert_eq!(off_lock, blocking, "same blob");
    assert!(
        under_lock * 3 < whole,
        "the locked part ({under_lock:?}) must be a fraction of the whole export ({whole:?})"
    );
    let restored = SyncCore::restore(&off_lock, 2).unwrap();
    assert_eq!(restored.item_ids(page).len(), 3_000);
}
