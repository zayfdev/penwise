//! Frame-level behaviour of `SyncCore` around refusals, catch-up, server changes and oversized ops (no server:
//! the test plays the server by encoding `ServerMsg`s).
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

use penwise_core::edit::Edit;
use penwise_core::item::{ItemId, Layer};
use penwise_ffi::error::SyncError;
use penwise_ffi::sync_core::{NewPage, SyncCore};
use penwise_proto::{
    AnyOp, Background, ClientKind, ClientMsg, DeviceId, MAX_SKEW_MS, NotebookId, OpId, OpRejected,
    OpRejection, PageId, PageKind, SeqOp, ServerId, ServerMsg, decode_client, encode_server,
};
use penwise_testkit::stroke_item;

const SERVER_NOW: u64 = 10_000_000;

fn core(seed: u64) -> SyncCore {
    SyncCore::new(DeviceId::from_parts(1_000, u128::from(seed)), seed)
}

fn server_a() -> ServerId {
    ServerId::from_parts(1, 9)
}

fn welcome(server: ServerId, head: u64) -> Vec<u8> {
    encode_server(&ServerMsg::Welcome {
        protocol_version: penwise_proto::PROTOCOL_VERSION,
        server,
        device: DeviceId::from_parts(1_000, 1),
        head_seq: head,
        log_floor: 0,
        server_now_ms: SERVER_NOW,
        current_page: None,
    })
    .unwrap()
}

fn notebook_and_page(core: &mut SyncCore, now: u64) -> (NotebookId, PageId) {
    let notebook = core.create_notebook(None, "nb", 0, now).unwrap();
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
            now,
        )
        .unwrap();
    (notebook, page)
}

fn stroke(id: u8, points: usize) -> Edit {
    let line: Vec<[f32; 2]> = (0..points).map(|i| [i as f32 * 0.5, 3.0]).collect();
    Edit::Add(stroke_item(ItemId([id; 16]), 65_536, Layer::Ink, 7, &line))
}

fn connect(core: &mut SyncCore, head: u64) {
    core.hello("t", ClientKind::Tablet, SERVER_NOW).unwrap();
    core.receive(&welcome(server_a(), head), SERVER_NOW)
        .unwrap();
}

fn pushed(core: &mut SyncCore) -> Vec<AnyOp> {
    match decode_client(&core.poll_push().unwrap().expect("a push")).unwrap() {
        ClientMsg::PushOps { ops } => ops,
        other => panic!("not a push: {other:?}"),
    }
}

fn ack(
    core: &mut SyncCore,
    ops: &[AnyOp],
    refuse: &[(OpId, OpRejection)],
) -> penwise_ffi::sync_core::Outcome {
    let rejected: Vec<OpRejected> = refuse
        .iter()
        .map(|&(op_id, reason)| OpRejected { op_id, reason })
        .collect();
    let op_ids: Vec<OpId> = ops
        .iter()
        .map(AnyOp::op_id)
        .filter(|id| !refuse.iter().any(|(r, _)| r == id))
        .collect();
    let seqs = (1..=op_ids.len() as u64).collect();
    let frame = encode_server(&ServerMsg::Ack {
        op_ids,
        seqs,
        rejected,
    })
    .unwrap();
    core.receive(&frame, SERVER_NOW).unwrap()
}

fn ops_since(ops: &[AnyOp], from: u64) -> Vec<u8> {
    let seq_ops: Vec<SeqOp> = ops
        .iter()
        .zip(from + 1..)
        .map(|(op, seq)| SeqOp {
            seq,
            op: op.clone(),
        })
        .collect();
    let upto = from + seq_ops.len() as u64;
    encode_server(&ServerMsg::OpsSince {
        from_seq: from,
        upto_seq: upto,
        ops: seq_ops,
        head_seq: upto,
        more: false,
    })
    .unwrap()
}

#[test]
fn a_refused_op_leaves_the_replica_and_is_listed_as_a_dead_letter() {
    let mut core = core(1);
    let (_nb, page) = notebook_and_page(&mut core, SERVER_NOW);
    connect(&mut core, 0);
    let catalog = pushed(&mut core);
    assert!(ack(&mut core, &catalog, &[]).rejected == 0);
    // Server state the replica gets back after the rebuild.
    let server_log = catalog.clone();

    core.commit_edits(page, &[stroke(1, 10)], SERVER_NOW)
        .unwrap();
    assert_eq!(core.item_ids(page).len(), 1);
    let batch = pushed(&mut core);
    let bad = batch[0].op_id();
    let out = ack(&mut core, &batch, &[(bad, OpRejection::Invalid)]);
    assert_eq!(out.rejected, 1);
    assert!(out.reconnect, "the host must resync");
    assert!(core.outbox().is_empty());

    let dead = core.dead_letters();
    assert_eq!(dead.len(), 1);
    assert_eq!(
        (dead[0].op_id, dead[0].kind, dead[0].page),
        (bad, 0, Some(page))
    );
    assert_eq!(dead[0].reason, 2);
    assert!(
        core.dead_letter_bytes(bad)
            .is_some_and(|b| b.len() == dead[0].size as usize)
    );
    assert!(!core.is_caught_up());

    // After the resync the server's state is back and the refused ink is not.
    core.hello("t", ClientKind::Tablet, SERVER_NOW).unwrap();
    core.receive(&welcome(server_a(), 2), SERVER_NOW).unwrap();
    core.receive(&ops_since(&server_log, 0), SERVER_NOW)
        .unwrap();
    assert!(core.replica().page(page).is_some());
    assert!(core.item_ids(page).is_empty(), "refused ink is gone");
    assert!(core.remove_dead_letter(bad));
    assert!(core.dead_letters().is_empty());
}

#[test]
fn clock_skew_refusals_are_restamped_and_retried_once() {
    let mut core = core(2);
    // Offline work with a clock ten minutes fast (before any calibration).
    let (_nb, page) = notebook_and_page(&mut core, SERVER_NOW + 2 * MAX_SKEW_MS);
    core.commit_edits(page, &[stroke(1, 6)], SERVER_NOW + 2 * MAX_SKEW_MS)
        .unwrap();
    connect(&mut core, 0);
    let first = pushed(&mut core);
    assert_eq!(first.len(), 3);
    let refused: Vec<_> = first
        .iter()
        .map(|op| (op.op_id(), OpRejection::ClockSkew))
        .collect();
    let out = ack(&mut core, &first, &refused);
    assert_eq!(out.rejected, 0, "skew is retried, not dead-lettered");
    assert!(out.reconnect);
    assert!(core.dead_letters().is_empty());
    assert_eq!(core.outbox().len(), 3);

    connect(&mut core, 0);
    let second = pushed(&mut core);
    assert_eq!(
        second.iter().map(AnyOp::op_id).collect::<Vec<_>>(),
        first.iter().map(AnyOp::op_id).collect::<Vec<_>>(),
        "same ops, same order"
    );
    for op in &second {
        assert!(
            op.hlc().wall_ms <= SERVER_NOW + MAX_SKEW_MS,
            "re-stamped within the accepted window"
        );
    }
    // Refused for skew again before any clean Ack: final.
    let refused: Vec<_> = second
        .iter()
        .map(|op| (op.op_id(), OpRejection::ClockSkew))
        .collect();
    let out = ack(&mut core, &second, &refused);
    assert_eq!(out.rejected, 3);
    assert_eq!(core.dead_letters().len(), 3);
    assert!(core.outbox().is_empty());
}

#[test]
fn a_duplicate_refusal_means_the_server_has_it() {
    let mut core = core(3);
    let (_nb, _page) = notebook_and_page(&mut core, SERVER_NOW);
    connect(&mut core, 0);
    let batch = pushed(&mut core);
    let dup = batch[0].op_id();
    let out = ack(&mut core, &batch, &[(dup, OpRejection::Duplicate)]);
    assert_eq!(out.rejected, 0);
    assert!(out.reconnect);
    assert!(core.dead_letters().is_empty());
    assert!(core.outbox().is_empty());
}

#[test]
fn caught_up_follows_the_welcome_head() {
    let mut core = core(4);
    assert!(!core.is_caught_up());
    core.hello("t", ClientKind::Tablet, SERVER_NOW).unwrap();
    core.receive(&welcome(server_a(), 5), SERVER_NOW).unwrap();
    assert!(!core.is_caught_up(), "behind the Welcome head");
    // Five ops: a notebook, a page and three of the same page are enough; use a real log.
    let mut other = self::core(9);
    let (_nb, page) = notebook_and_page(&mut other, 1_000);
    other
        .commit_edits(page, &[stroke(1, 4), stroke(2, 4), stroke(3, 4)], 1_000)
        .unwrap();
    let log: Vec<AnyOp> = other.outbox().pending_ops();
    assert_eq!(log.len(), 5);
    core.receive(&ops_since(&log[..2], 0), SERVER_NOW).unwrap();
    assert!(!core.is_caught_up());
    core.receive(&ops_since(&log[2..], 2), SERVER_NOW).unwrap();
    assert!(core.is_caught_up());
    core.go_offline();
    assert!(!core.is_caught_up());
    // Nothing to catch up on.
    let mut empty = self::core(5);
    empty.hello("t", ClientKind::Tablet, SERVER_NOW).unwrap();
    empty.receive(&welcome(server_a(), 0), SERVER_NOW).unwrap();
    assert!(empty.is_caught_up());
}

#[test]
fn an_oversized_op_is_refused_locally_and_changes_nothing() {
    let mut core = core(6);
    let (_nb, page) = notebook_and_page(&mut core, SERVER_NOW);
    let before = core.outbox().len();
    let err = core
        .commit_edits(page, &[stroke(1, 400_000)], SERVER_NOW)
        .unwrap_err();
    assert!(matches!(err, SyncError::OpTooLarge(_)), "{err}");
    assert_eq!(core.outbox().len(), before);
    assert!(core.item_ids(page).is_empty());
    // A normal stroke still goes through afterwards.
    core.commit_edits(page, &[stroke(2, 50)], SERVER_NOW)
        .unwrap();
    assert_eq!(core.outbox().len(), before + 1);
}

#[test]
fn a_different_server_is_held_until_the_user_decides() {
    for keep in [true, false] {
        let mut core = core(7);
        let (nb, page) = notebook_and_page(&mut core, SERVER_NOW);
        core.commit_edits(page, &[stroke(1, 8), stroke(2, 8)], SERVER_NOW)
            .unwrap();
        connect(&mut core, 0);
        let acked = pushed(&mut core);
        assert!(ack(&mut core, &acked, &[]).rejected == 0);
        assert!(core.outbox().is_empty());
        // One more op stays unsent (offline), then the other server answers.
        core.commit_edits(page, &[stroke(3, 8)], SERVER_NOW)
            .unwrap();
        core.go_offline();
        core.hello("t", ClientKind::Tablet, SERVER_NOW).unwrap();
        let out = core
            .receive(&welcome(ServerId::from_parts(2, 2), 0), SERVER_NOW)
            .unwrap();
        assert!(out.server_changed && !out.reconnect);
        assert!(core.server_change_pending());
        assert!(core.poll_push().unwrap().is_none(), "no push while held");
        let ignored = core
            .receive(&ops_since(&acked[..1], 0), SERVER_NOW)
            .unwrap();
        assert!(ignored.server_changed);
        assert_eq!(core.head_seq(), 0, "nothing applied");
        assert_eq!(core.item_ids(page).len(), 3, "nothing dropped while held");
        assert!(!core.is_caught_up());

        let out = core.resolve_server_change(keep, SERVER_NOW).unwrap();
        assert!(out.reconnect && out.catalog_changed);
        assert!(!core.server_change_pending());
        assert!(core.resolve_server_change(keep, SERVER_NOW).is_err());
        if keep {
            // notebook + page + 3 strokes queued again for the new server, all content kept.
            assert_eq!(core.outbox().len(), 5);
            assert_eq!(core.item_ids(page).len(), 3);
            assert!(core.replica().catalog().notebook(nb).is_some());
            // Deterministic ids: carrying the same state again queues the same op ids.
            let first: Vec<OpId> = core
                .outbox()
                .pending_ops()
                .iter()
                .map(AnyOp::op_id)
                .collect();
            let mut again = SyncCore::restore(&core.export_state().unwrap(), 1).unwrap();
            let held = ServerId::from_parts(3, 3);
            again.hello("t", ClientKind::Tablet, SERVER_NOW).unwrap();
            again.receive(&welcome(held, 0), SERVER_NOW).unwrap();
            assert!(again.server_change_pending());
            again.resolve_server_change(true, SERVER_NOW + 5).unwrap();
            let second: Vec<OpId> = again
                .outbox()
                .pending_ops()
                .iter()
                .map(AnyOp::op_id)
                .collect();
            assert_eq!(first, second);
        } else {
            // Only what was never sent survives (its page is gone, so it cannot apply).
            assert_eq!(core.outbox().len(), 1);
            assert!(core.item_ids(page).is_empty());
        }
        // The new server is pinned: it now welcomes normally.
        connect_other(&mut core);
        assert!(!core.server_change_pending());
    }
}

fn connect_other(core: &mut SyncCore) {
    core.hello("t", ClientKind::Tablet, SERVER_NOW).unwrap();
    let out = core
        .receive(&welcome(ServerId::from_parts(2, 2), 0), SERVER_NOW)
        .unwrap();
    assert!(!out.server_changed);
}

fn pwst_one(seed: i64) -> Vec<u8> {
    let mut out = b"PWST".to_vec();
    out.push(1);
    out.push(1);
    out.extend(0_i32.to_be_bytes());
    out.extend((-16_777_216_i32).to_be_bytes());
    out.extend(1.0_f32.to_be_bytes());
    out.extend(seed.to_be_bytes());
    out.extend(3_i32.to_be_bytes());
    for i in 0..3_i32 {
        for v in [10.0_f32 + i as f32, 5.0, 0.5, 0.0, 0.0] {
            out.extend(v.to_be_bytes());
        }
        out.extend((i * 8_000).to_be_bytes());
    }
    out
}

#[test]
fn the_servers_version_of_an_imported_legacy_row_replaces_the_local_one() {
    use penwise_proto::legacy::{
        LegacyStroke, legacy_add_op, legacy_item_id, legacy_notebook_id, legacy_page_id,
        stroke_to_item,
    };
    use penwise_proto::{Hlc, NotebookChange, NotebookOp, Op, PageChange, PageOp};

    let mut core = core(8);
    let notebook = core
        .create_notebook(Some(legacy_notebook_id("nb_1")), "local title", 0, 5_000)
        .unwrap();
    let page = legacy_page_id("pg_1");
    core.create_page(
        Some(page),
        NewPage {
            notebook,
            title: "local".to_owned(),
            position: 0,
            kind: PageKind::Infinite,
            background: Background::Blank,
        },
        5_000,
    )
    .unwrap();
    let report = core
        .import_legacy_page("pg_1", &pwst_one(5), 5_000)
        .unwrap();
    assert_eq!(report.added, 1);
    let item_id = legacy_item_id("pg_1", "5");

    // The server's migration of the same rows: same ids, its own device and stamps, then an erase.
    let server_device = DeviceId::from_parts(9, 9);
    let at = |wall_ms| Hlc {
        wall_ms,
        counter: 0,
        device: server_device,
    };
    let legacy = LegacyStroke {
        layer: "ink".to_owned(),
        preset: 0,
        argb: -16_777_216,
        scale: 1.0,
        seed: "5".to_owned(),
        samples: vec![10.0, 5.0, 0.5, 0.0, 0.0],
        times: vec![0],
    };
    let log = vec![
        AnyOp::Notebook(NotebookOp {
            op_id: OpId::from_parts(1, 1),
            device: server_device,
            hlc: at(1_000),
            notebook,
            change: NotebookChange::Create {
                title: "server title".to_owned(),
                position: 3,
            },
        }),
        AnyOp::Page(PageOp {
            op_id: OpId::from_parts(1, 2),
            device: server_device,
            hlc: at(1_001),
            page,
            change: PageChange::Create {
                notebook,
                title: "server".to_owned(),
                position: 0,
                kind: PageKind::Infinite,
                background: Background::Blank,
            },
        }),
        legacy_add_op(
            page,
            server_device,
            at(1_002),
            stroke_to_item("pg_1", &legacy, 65_536).unwrap(),
        ),
        AnyOp::Item(Box::new(Op {
            op_id: OpId::from_parts(1, 3),
            device: server_device,
            hlc: at(2_000),
            page,
            edit: Edit::Delete(item_id),
        })),
    ];
    core.hello("t", ClientKind::Tablet, 6_000).unwrap();
    core.receive(&welcome(server_a(), 4), 6_000).unwrap();
    let out = core.receive(&ops_since(&log, 0), 6_000).unwrap();
    assert_eq!(
        out.apply_errors, 0,
        "legacy duplicates are adopted, not errors"
    );
    assert_eq!(core.head_seq(), 4);
    let state = core.replica().page(page).unwrap().item(item_id).unwrap();
    assert_eq!(
        state.stamps.created,
        at(1_002),
        "the server's creation stamp"
    );
    assert!(state.deleted, "the erase made elsewhere applies");
    assert_eq!(
        core.replica().catalog().notebook(notebook).unwrap().title,
        "server title"
    );
    assert!(core.is_caught_up());
}
