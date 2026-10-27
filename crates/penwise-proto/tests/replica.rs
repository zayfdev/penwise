//! Replica semantics: per-field LWW, tombstones, Add duplicate rules, create-before-reference, cursor.
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
    device, hlc, item_id, item_op, notebook_id, notebook_op, op_id, page_id, page_op, text_item,
};
use penwise_core::edit::{Edit, EditError, Field, ItemPatch};
use penwise_core::item::{Affine, ItemId, Layer};
use penwise_proto::{
    AnyOp, Background, HlcClock, NotebookChange, NotebookPatch, Op, PageChange, PageId, PageKind,
    PagePatch, Replica, ReplicaError, SeqOp,
};

const PAGE: u64 = 1;

fn kind() -> PageKind {
    PageKind::Fixed {
        width: 1000.0,
        height: 800.0,
    }
}

/// A replica holding notebook 1 and page 1, authored by device 9.
fn world() -> Replica {
    let mut clock = HlcClock::new(device(9));
    let mut replica = Replica::new();
    let ops = [
        notebook_op(
            &mut clock,
            1_000,
            1,
            notebook_id(1),
            NotebookChange::Create {
                title: "Notes".into(),
                position: 0,
            },
        ),
        page_op(
            &mut clock,
            1_000,
            2,
            page_id(PAGE),
            PageChange::Create {
                notebook: notebook_id(1),
                title: String::new(),
                position: 65_536,
                kind: kind(),
                background: Background::Blank,
            },
        ),
    ];
    for op in &ops {
        replica.apply(op).unwrap();
    }
    replica
}

fn page() -> PageId {
    page_id(PAGE)
}

fn add(clock: &mut HlcClock, now: u64, op: u64, id: ItemId, z: i64) -> AnyOp {
    item_op(clock, now, op, page(), Edit::Add(text_item(id, z, "hi")))
}

fn update(clock: &mut HlcClock, now: u64, op: u64, id: ItemId, patch: ItemPatch) -> AnyOp {
    item_op(clock, now, op, page(), Edit::Update { id, patch })
}

fn moved(x: f32) -> ItemPatch {
    ItemPatch {
        transform: Some(Affine::translate(x, 0.0)),
        ..ItemPatch::default()
    }
}

fn state(replica: &Replica, id: ItemId) -> (Affine, bool) {
    let item = replica.page(page()).unwrap().item(id).unwrap();
    (item.item.transform, item.deleted)
}

#[test]
fn add_applies_once_and_the_authors_echo_is_a_no_op() {
    let mut replica = world();
    let mut clock = HlcClock::new(device(1));
    let op = add(&mut clock, 2_000, 10, item_id(1), 65_536);
    let applied = replica.apply(&op).unwrap();
    assert_eq!(applied.page, Some(page()));
    assert_eq!(applied.item, Some(item_id(1)));
    assert!(!applied.catalog_changed);
    // Echo of the same op (same id, same stamp): nothing changes, nothing is duplicated.
    let echo = replica.apply(&op).unwrap();
    assert_eq!((echo.page, echo.item), (None, None));
    assert_eq!(replica.page(page()).unwrap().scene().len(), 1);
}

#[test]
fn add_of_an_existing_id_with_another_stamp_is_a_duplicate() {
    let mut replica = world();
    let mut a = HlcClock::new(device(1));
    let mut b = HlcClock::new(device(2));
    replica
        .apply(&add(&mut a, 2_000, 10, item_id(1), 65_536))
        .unwrap();
    let other = add(&mut b, 2_000, 11, item_id(1), 131_072);
    assert_eq!(replica.apply(&other), Err(ReplicaError::Duplicate));
    let kept = replica.page(page()).unwrap().item(item_id(1)).unwrap();
    assert_eq!(kept.item.z, 65_536, "first Add must stay untouched");
}

#[test]
fn invalid_add_is_rejected_and_leaves_no_trace() {
    let mut replica = world();
    let mut clock = HlcClock::new(device(1));
    let mut bad = text_item(item_id(1), 0, "x");
    if let penwise_core::item::ItemKind::Text(t) = &mut bad.kind {
        t.font_size = -1.0;
    }
    let op = item_op(&mut clock, 2_000, 10, page(), Edit::Add(bad));
    assert!(matches!(
        replica.apply(&op),
        Err(ReplicaError::Edit(EditError::InvalidValue { .. }))
    ));
    assert!(replica.page(page()).unwrap().item(item_id(1)).is_none());
}

#[test]
fn update_registers_are_independent_and_newest_stamp_wins_per_field() {
    let mut replica = world();
    let mut a = HlcClock::new(device(1));
    let mut b = HlcClock::new(device(2));
    replica
        .apply(&add(&mut a, 2_000, 10, item_id(1), 65_536))
        .unwrap();
    // B writes transform at t=3000 and z at t=3000; A writes transform at t=4000 only.
    let b_patch = ItemPatch {
        transform: Some(Affine::translate(5.0, 0.0)),
        z: Some(7),
        ..ItemPatch::default()
    };
    let b_op = update(&mut b, 3_000, 11, item_id(1), b_patch);
    let a_op = update(&mut a, 4_000, 12, item_id(1), moved(9.0));
    // Deliver the newer transform first: the older write must not undo it, but its z still lands.
    replica.apply(&a_op).unwrap();
    let late = replica.apply(&b_op).unwrap();
    assert_eq!(
        late.item,
        Some(item_id(1)),
        "z of the older op still wins its own register"
    );
    let item = replica.page(page()).unwrap().item(item_id(1)).unwrap();
    assert_eq!(item.item.transform, Affine::translate(9.0, 0.0));
    assert_eq!(item.item.z, 7);
    // The scene follows the materialized state.
    let scene_item = replica
        .page(page())
        .unwrap()
        .scene()
        .get(item_id(1))
        .unwrap();
    assert_eq!(scene_item.transform, Affine::translate(9.0, 0.0));
    assert_eq!(scene_item.z, 7);
}

#[test]
fn stale_update_is_ignored_entirely() {
    let mut replica = world();
    let mut a = HlcClock::new(device(1));
    let mut b = HlcClock::new(device(2));
    replica
        .apply(&add(&mut a, 2_000, 10, item_id(1), 65_536))
        .unwrap();
    replica
        .apply(&update(&mut a, 5_000, 11, item_id(1), moved(9.0)))
        .unwrap();
    let stale = update(&mut b, 4_000, 12, item_id(1), moved(1.0));
    let applied = replica.apply(&stale).unwrap();
    assert_eq!((applied.page, applied.item), (None, None));
    assert_eq!(state(&replica, item_id(1)).0, Affine::translate(9.0, 0.0));
}

#[test]
fn delete_and_update_are_independent_and_undelete_keeps_the_latest_transform() {
    let mut replica = world();
    let mut a = HlcClock::new(device(1));
    let mut b = HlcClock::new(device(2));
    replica
        .apply(&add(&mut a, 2_000, 10, item_id(1), 65_536))
        .unwrap();
    // A deletes at t=3000; B moves at t=4000 (never saw the delete).
    replica
        .apply(&item_op(
            &mut a,
            3_000,
            11,
            page(),
            Edit::Delete(item_id(1)),
        ))
        .unwrap();
    replica
        .apply(&update(&mut b, 4_000, 12, item_id(1), moved(3.0)))
        .unwrap();
    assert_eq!(
        state(&replica, item_id(1)),
        (Affine::translate(3.0, 0.0), true)
    );
    assert!(
        replica
            .page(page())
            .unwrap()
            .scene()
            .get(item_id(1))
            .is_none()
    );
    // Undelete (Update{deleted:false}) later: restored at its latest transform.
    let undelete = ItemPatch {
        deleted: Some(false),
        ..ItemPatch::default()
    };
    replica
        .apply(&update(&mut a, 5_000, 13, item_id(1), undelete))
        .unwrap();
    assert_eq!(
        state(&replica, item_id(1)),
        (Affine::translate(3.0, 0.0), false)
    );
    let restored = replica
        .page(page())
        .unwrap()
        .scene()
        .get(item_id(1))
        .unwrap();
    assert_eq!(restored.transform, Affine::translate(3.0, 0.0));
}

#[test]
fn an_older_delete_does_not_override_a_newer_undelete() {
    let mut replica = world();
    let mut a = HlcClock::new(device(1));
    let mut b = HlcClock::new(device(2));
    replica
        .apply(&add(&mut a, 2_000, 10, item_id(1), 65_536))
        .unwrap();
    let del = item_op(&mut b, 3_000, 11, page(), Edit::Delete(item_id(1)));
    let undel = update(
        &mut a,
        4_000,
        12,
        item_id(1),
        ItemPatch {
            deleted: Some(false),
            ..ItemPatch::default()
        },
    );
    replica.apply(&undel).unwrap();
    replica.apply(&del).unwrap();
    assert!(
        !state(&replica, item_id(1)).1,
        "the older Delete loses to the newer Update"
    );
}

#[test]
fn patch_fields_the_kind_lacks_are_rejected_whatever_their_stamp() {
    let mut replica = world();
    let mut a = HlcClock::new(device(1));
    replica
        .apply(&add(&mut a, 2_000, 10, item_id(1), 65_536))
        .unwrap();
    let wrong = ItemPatch {
        shape_size: Some([1.0, 1.0]),
        transform: Some(Affine::translate(1.0, 1.0)),
        ..ItemPatch::default()
    };
    let op = update(&mut a, 3_000, 11, item_id(1), wrong);
    assert_eq!(
        replica.apply(&op),
        Err(ReplicaError::Edit(EditError::FieldNotApplicable {
            id: item_id(1),
            field: Field::ShapeSize
        }))
    );
    assert_eq!(
        state(&replica, item_id(1)).0,
        Affine::IDENTITY,
        "atomic: transform not applied either"
    );
}

#[test]
fn layer_update_round_trips_through_the_replica() {
    let mut replica = world();
    let mut a = HlcClock::new(device(1));
    replica
        .apply(&add(&mut a, 2_000, 10, item_id(1), 65_536))
        .unwrap();
    let patch = ItemPatch {
        layer: Some(Layer::Agent),
        ..ItemPatch::default()
    };
    replica
        .apply(&update(&mut a, 3_000, 11, item_id(1), patch))
        .unwrap();
    let item = replica.page(page()).unwrap().item(item_id(1)).unwrap();
    assert_eq!(item.item.layer, Layer::Agent);
}

#[test]
fn create_before_reference_is_enforced() {
    let mut replica = Replica::new();
    let mut clock = HlcClock::new(device(1));
    let orphan_item = add(&mut clock, 1_000, 1, item_id(1), 0);
    assert_eq!(replica.apply(&orphan_item), Err(ReplicaError::UnknownPage));
    let orphan_page = page_op(
        &mut clock,
        1_000,
        2,
        page(),
        PageChange::Create {
            notebook: notebook_id(1),
            title: String::new(),
            position: 0,
            kind: kind(),
            background: Background::Blank,
        },
    );
    assert_eq!(
        replica.apply(&orphan_page),
        Err(ReplicaError::UnknownNotebook)
    );
    let page_update = page_op(
        &mut clock,
        1_000,
        3,
        page(),
        PageChange::Update(PagePatch {
            deleted: Some(true),
            ..PagePatch::default()
        }),
    );
    assert_eq!(replica.apply(&page_update), Err(ReplicaError::UnknownPage));
    let nb_update = notebook_op(
        &mut clock,
        1_000,
        4,
        notebook_id(1),
        NotebookChange::Update(NotebookPatch {
            deleted: Some(true),
            ..NotebookPatch::default()
        }),
    );
    assert_eq!(
        replica.apply(&nb_update),
        Err(ReplicaError::UnknownNotebook)
    );

    let mut replica = world();
    let ghost = update(&mut clock, 2_000, 5, item_id(77), moved(1.0));
    assert_eq!(replica.apply(&ghost), Err(ReplicaError::UnknownItem));
}

#[test]
fn items_are_accepted_under_a_tombstoned_page() {
    let mut replica = world();
    let mut a = HlcClock::new(device(1));
    let delete_page = page_op(
        &mut a,
        2_000,
        10,
        page(),
        PageChange::Update(PagePatch {
            deleted: Some(true),
            ..PagePatch::default()
        }),
    );
    let applied = replica.apply(&delete_page).unwrap();
    assert!(applied.catalog_changed);
    assert!(replica.catalog().pages(notebook_id(1)).is_empty());
    assert_eq!(replica.catalog().deleted_pages(notebook_id(1)).len(), 1);
    // An offline device adds ink to the now-deleted page: kept, no rejection, page stays deleted.
    replica
        .apply(&add(&mut a, 2_500, 11, item_id(1), 65_536))
        .unwrap();
    assert_eq!(replica.page(page()).unwrap().scene().len(), 1);
    assert!(replica.catalog().page(page()).unwrap().deleted);
}

#[test]
fn catalog_registers_are_lww_and_lists_are_ordered() {
    let mut replica = world();
    let mut a = HlcClock::new(device(1));
    let mut b = HlcClock::new(device(2));
    // Second page at an earlier position.
    replica
        .apply(&page_op(
            &mut a,
            2_000,
            10,
            page_id(2),
            PageChange::Create {
                notebook: notebook_id(1),
                title: "first".into(),
                position: 1,
                kind: PageKind::Infinite,
                background: Background::Grid,
            },
        ))
        .unwrap();
    let order: Vec<_> = replica
        .catalog()
        .pages(notebook_id(1))
        .iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(order, vec![page_id(2), page_id(1)]);
    // Concurrent retitle: the later stamp wins, in either arrival order.
    let early = page_op(
        &mut a,
        3_000,
        11,
        page(),
        PageChange::Update(PagePatch {
            title: Some("early".into()),
            ..PagePatch::default()
        }),
    );
    let late = page_op(
        &mut b,
        4_000,
        12,
        page(),
        PageChange::Update(PagePatch {
            title: Some("late".into()),
            background: Some(Background::Dot),
            ..PagePatch::default()
        }),
    );
    replica.apply(&late).unwrap();
    let echo = replica.apply(&early).unwrap();
    assert!(!echo.catalog_changed);
    let p = replica.catalog().page(page()).unwrap();
    assert_eq!((p.title.as_str(), p.background), ("late", Background::Dot));
    // A second Create of the same id under another stamp is refused.
    let again = replica
        .apply(&page_op(
            &mut a,
            9_000,
            99,
            page(),
            PageChange::Create {
                notebook: notebook_id(1),
                title: String::new(),
                position: 0,
                kind: kind(),
                background: Background::Blank,
            },
        ))
        .unwrap_err();
    assert_eq!(
        again,
        ReplicaError::Duplicate,
        "same id, different stamp is a duplicate"
    );
}

#[test]
fn apply_seq_skips_seen_seqs_and_advances_even_on_failure() {
    let mut replica = world();
    let mut a = HlcClock::new(device(1));
    let good = SeqOp {
        seq: 5,
        op: add(&mut a, 2_000, 10, item_id(1), 65_536),
    };
    assert!(replica.apply_seq(&good).unwrap().page.is_some());
    assert_eq!(replica.head_seq(), 5);
    // Already seen: skipped without touching state.
    let skipped = replica.apply_seq(&good).unwrap();
    assert_eq!(skipped.page, None);
    let old = SeqOp {
        seq: 3,
        op: add(&mut a, 2_100, 11, item_id(2), 65_536),
    };
    replica.apply_seq(&old).unwrap();
    assert!(replica.page(page()).unwrap().item(item_id(2)).is_none());
    // A failing op still moves the cursor so sync is never wedged.
    let bad = SeqOp {
        seq: 6,
        op: AnyOp::Item(Box::new(Op {
            op_id: op_id(50),
            device: device(1),
            hlc: hlc(9_000, 0, 1),
            page: page_id(404),
            edit: Edit::Delete(item_id(1)),
        })),
    };
    assert_eq!(replica.apply_seq(&bad), Err(ReplicaError::UnknownPage));
    assert_eq!(replica.head_seq(), 6);
    // Filtered batches advance the cursor across the gap, never backwards.
    replica.advance_to(40);
    replica.advance_to(10);
    assert_eq!(replica.head_seq(), 40);
}
