//! Client recovery building blocks: clock adoption and rebase, snapshot stamp observation, carrying a replica
//! to another server, adopting a server's create, rebuilding from pending ops, the per-op size cap.
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

use common::{
    device, hlc, item_id, item_op, notebook_id, notebook_op, op_id, page_id, page_op, stroke_item,
};
use penwise_core::edit::Edit;
use penwise_proto::carry::carry_ops;
use penwise_proto::outbox::{MAX_OP_BYTES, check_edit_size};
use penwise_proto::{
    AnyOp, Background, HlcClock, HlcError, ItemState, MAX_SKEW_MS, NotebookChange, Outbox,
    PageChange, PageKind, ProtoError, Replica, Scope, SnapshotChunk, observe_snapshot_chunk,
};

fn library(replica: &mut Replica) -> (penwise_proto::NotebookId, penwise_proto::PageId) {
    let mut clock = HlcClock::new(device(1));
    let (nb, page) = (notebook_id(1), page_id(1));
    let ops = [
        notebook_op(
            &mut clock,
            1_000,
            1,
            nb,
            NotebookChange::Create {
                title: "n".to_owned(),
                position: 2,
            },
        ),
        page_op(
            &mut clock,
            1_000,
            2,
            page,
            PageChange::Create {
                notebook: nb,
                title: "p".to_owned(),
                position: 0,
                kind: PageKind::Infinite,
                background: Background::Grid,
            },
        ),
        item_op(
            &mut clock,
            1_000,
            3,
            page,
            Edit::Add(stroke_item(item_id(1), 65_536, 1)),
        ),
        item_op(
            &mut clock,
            1_000,
            4,
            page,
            Edit::Add(stroke_item(item_id(2), 131_072, 2)),
        ),
        item_op(&mut clock, 1_001, 5, page, Edit::Delete(item_id(2))),
    ];
    for op in &ops {
        replica.apply(op).unwrap();
    }
    (nb, page)
}

#[test]
fn observe_logged_adopts_a_stamp_far_ahead_and_reports_it() {
    let mut clock = HlcClock::new(device(1));
    let remote = hlc(1_000_000 + 2 * MAX_SKEW_MS, 4, 2);
    assert!(matches!(
        clock.observe_logged(remote, 1_000_000),
        Err(HlcError::ClockSkew { .. })
    ));
    assert!(
        clock.send(1_000_000) > remote,
        "later stamps order after the logged one"
    );
    // `recv` alone would have left the clock behind.
    let mut plain = HlcClock::new(device(1));
    assert!(plain.recv(remote, 1_000_000).is_err());
    assert!(plain.send(1_000_000) < remote);
}

#[test]
fn rebase_drops_a_clock_that_ran_ahead_but_not_below_the_floor() {
    let mut clock = HlcClock::new(device(1));
    let ahead = clock.send(9_000_000);
    let floor = hlc(5_000, 3, 2);
    clock.rebase(floor, 1_000);
    let next = clock.send(1_000);
    assert!(next < ahead);
    assert!(next > floor, "still after everything observed");
    assert_eq!(next.device, device(1));
}

#[test]
fn snapshot_chunks_feed_the_clock() {
    let mut replica = Replica::new();
    library(&mut replica);
    let chunk = SnapshotChunk {
        notebooks: replica.catalog().notebook_states(),
        pages: replica.catalog().page_states(),
        items: replica
            .page(page_id(1))
            .unwrap()
            .items()
            .map(|m| ItemState::from_item(page_id(1), m))
            .collect(),
    };
    let newest = penwise_proto::newest_stamp(&chunk).unwrap();
    assert_eq!(newest.wall_ms, 1_001);
    let mut clock = HlcClock::new(device(9));
    observe_snapshot_chunk(&mut clock, &chunk, 5_000).unwrap();
    assert!(clock.send(500) > newest);
}

#[test]
fn a_page_snapshot_does_not_move_the_cursor() {
    let mut replica = Replica::new();
    replica.end_snapshot(Scope::Page(page_id(1)), 50);
    assert_eq!(replica.head_seq(), 0);
    replica.end_snapshot(Scope::Catalog, 50);
    assert_eq!(replica.head_seq(), 50);
}

#[test]
fn carried_ops_rebuild_the_live_content_with_deterministic_ids() {
    let mut replica = Replica::new();
    let (nb, page) = library(&mut replica);
    let mut clock = HlcClock::new(device(7));
    let ops = carry_ops(&replica, device(7), || clock.send(9_000));
    // notebook, page, one live item (the deleted one is not carried).
    assert_eq!(ops.len(), 3);
    let mut rebuilt = Replica::new();
    rebuilt.rebuild_from(&ops);
    assert!(rebuilt.catalog().notebook(nb).is_some());
    assert_eq!(rebuilt.page(page).unwrap().items().count(), 1);
    assert!(rebuilt.page(page).unwrap().item(item_id(1)).is_some());

    let mut clock2 = HlcClock::new(device(7));
    let again = carry_ops(&replica, device(7), || clock2.send(77_000));
    let ids = |ops: &[AnyOp]| ops.iter().map(AnyOp::op_id).collect::<Vec<_>>();
    assert_eq!(ids(&ops), ids(&again), "op ids depend on the entity only");
    assert_ne!(ops[0].hlc(), again[0].hlc());
}

#[test]
fn adopt_create_replaces_a_local_creation_with_the_servers() {
    let mut replica = Replica::new();
    let (_nb, page) = library(&mut replica);
    let mut server = HlcClock::new(device(2));
    let theirs = item_op(
        &mut server,
        500,
        99,
        page,
        Edit::Add(stroke_item(item_id(1), 7_000, 9)),
    );
    assert!(replica.apply(&theirs).is_err(), "plain apply: Duplicate");
    replica.adopt_create(&theirs).unwrap();
    let state = replica.page(page).unwrap().item(item_id(1)).unwrap();
    assert_eq!(state.stamps.created, theirs.hlc());
    assert_eq!(state.item.z, 7_000);
    assert_eq!(
        replica.page(page).unwrap().scene().top_z(),
        7_000,
        "the scene follows"
    );
    assert!(
        replica.page(page).unwrap().item(item_id(2)).is_some(),
        "other items stay"
    );
}

#[test]
fn rebuild_from_keeps_only_the_given_ops() {
    let mut replica = Replica::new();
    let (nb, _page) = library(&mut replica);
    replica.advance_to(40);
    let mut clock = HlcClock::new(device(1));
    let keep = notebook_op(
        &mut clock,
        2_000,
        8,
        nb,
        NotebookChange::Create {
            title: "k".to_owned(),
            position: 0,
        },
    );
    replica.rebuild_from(&[keep]);
    assert_eq!(replica.head_seq(), 0);
    assert!(replica.page(page_id(1)).is_none());
    assert_eq!(replica.catalog().notebook(nb).unwrap().title, "k");
}

#[test]
fn the_outbox_refuses_ops_over_the_size_cap_and_replaces_its_pending_list() {
    let mut huge_points = Vec::new();
    let mut clock = HlcClock::new(device(1));
    let page = page_id(1);
    let mut item = stroke_item(item_id(1), 65_536, 1);
    if let penwise_core::item::ItemKind::Stroke(s) = &mut item.kind {
        let one = s.samples[0];
        huge_points.extend(std::iter::repeat_n(one, 400_000));
        s.samples = huge_points;
    }
    let edit = Edit::Add(item);
    assert!(check_edit_size(&edit).is_err());
    let big = item_op(&mut clock, 1_000, 1, page, edit);
    let mut outbox = Outbox::new();
    assert!(matches!(
        outbox.insert(&big),
        Err(ProtoError::TooLarge { .. })
    ));
    const { assert!(MAX_OP_BYTES < penwise_proto::MAX_FRAME_BYTES) };

    let small: Vec<AnyOp> = (1..=3)
        .map(|n| item_op(&mut clock, 1_000, n, page, Edit::Delete(item_id(n))))
        .collect();
    for op in &small {
        assert!(outbox.insert(op).unwrap());
    }
    let restamped: Vec<AnyOp> = small
        .iter()
        .map(|op| op.clone().with_hlc(hlc(9_000, 0, 1)))
        .collect();
    assert_eq!(outbox.replace_pending(&restamped), 0);
    assert_eq!(outbox.len(), 3);
    assert!(
        outbox
            .pending_ops()
            .iter()
            .all(|op| op.hlc().wall_ms == 9_000)
    );
    assert!(outbox.forget_pending(op_id(2)));
    assert_eq!(outbox.len(), 2);
    assert!(outbox.insert(&small[1]).unwrap(), "its id is free again");
}
