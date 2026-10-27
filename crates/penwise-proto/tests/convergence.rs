//! LWW convergence: any delivery order (respecting create-before-reference), with duplicates, gives equal
//! replicas, equal to a server-style `body ⊕ patch` fold and to a snapshot load.
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

use std::collections::BTreeMap;

use common::Rng;
use common::world::World;
use penwise_core::codec::item::encode_item;
use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::item::{Item, ItemId};
use penwise_proto::snapshot::encode_patch_blob;
use penwise_proto::{
    AnyOp, FieldStamps, ItemState, PageId, Replica, Scope, SnapshotChunk, merge_patch,
};

fn deliver(world: &World, order: &[usize]) -> Replica {
    let mut replica = Replica::new();
    for &i in order {
        replica
            .apply(&world.ops[i])
            .unwrap_or_else(|e| panic!("op {i} failed: {e}"));
    }
    replica
}

/// What the server stores per item: the creating `Add`'s item, the winning patch, stamps, tombstone.
struct Stored {
    body: Item,
    patch: ItemPatch,
    stamps: FieldStamps,
    deleted: bool,
}

fn server_fold(world: &World, order: &[usize]) -> BTreeMap<ItemId, Stored> {
    let mut rows: BTreeMap<ItemId, Stored> = BTreeMap::new();
    for &i in order {
        let AnyOp::Item(op) = &world.ops[i] else {
            continue;
        };
        let incoming = match &op.edit {
            Edit::Add(item) => {
                rows.entry(item.id).or_insert_with(|| Stored {
                    body: item.clone(),
                    patch: ItemPatch::default(),
                    stamps: FieldStamps::new(op.hlc),
                    deleted: false,
                });
                continue;
            }
            Edit::Delete(id) => (
                *id,
                ItemPatch {
                    deleted: Some(true),
                    ..ItemPatch::default()
                },
            ),
            Edit::Update { id, patch } => (*id, patch.clone()),
        };
        let row = rows.get_mut(&incoming.0).expect("create-before-reference");
        let mut winners = merge_patch(&mut row.stamps, &incoming.1, op.hlc);
        if let Some(deleted) = winners.deleted.take() {
            row.deleted = deleted;
        }
        row.patch.merge(&winners);
    }
    rows
}

fn assert_scenes_match_state(replica: &Replica, pages: &[PageId]) {
    for &page in pages {
        let page_replica = replica.page(page).expect("page exists");
        for state in page_replica.items() {
            let (item, deleted) = page_replica
                .scene()
                .get_any(state.item.id)
                .expect("scene has the item");
            assert_eq!(item, &state.item, "scene item must equal materialized item");
            assert_eq!(deleted, state.deleted);
        }
    }
}

#[test]
fn shuffled_and_duplicated_delivery_converges() {
    for seed in 0..40 {
        let world = World::random(seed, 24, 120);
        let creation: Vec<usize> = (0..world.ops.len()).collect();
        let reference = deliver(&world, &creation);
        let mut rng = Rng(seed ^ 0xABCD);
        for _ in 0..3 {
            let order = world.delivery(&mut rng, 30);
            let other = deliver(&world, &order);
            assert!(
                reference == other,
                "seed {seed}: replicas diverged under delivery {order:?}"
            );
        }
        let pages: Vec<PageId> = (1..=4).map(common::page_id).collect();
        assert_scenes_match_state(&reference, &pages);
    }
}

/// The generated worlds really exercise the rules: updated fields, tombstones, restored items, deleted pages.
#[test]
fn generated_worlds_are_not_vacuous() {
    let (mut updated, mut deleted, mut restored, mut dead_pages) = (0, 0, 0, 0);
    for seed in 0..40 {
        let world = World::random(seed, 24, 120);
        let creation: Vec<usize> = (0..world.ops.len()).collect();
        let replica = deliver(&world, &creation);
        for page in 1..=4 {
            for state in replica.page(common::page_id(page)).unwrap().items() {
                updated += usize::from(state.stamps != FieldStamps::new(state.stamps.created));
                deleted += usize::from(state.deleted);
                restored += usize::from(
                    !state.deleted
                        && state.stamps.get(penwise_core::edit::Field::Deleted)
                            != state.stamps.created,
                );
            }
            dead_pages += usize::from(
                replica
                    .catalog()
                    .page(common::page_id(page))
                    .unwrap()
                    .deleted,
            );
        }
    }
    assert!(
        updated > 300 && deleted > 100 && restored > 50 && dead_pages > 20,
        "{updated} {deleted} {restored} {dead_pages}"
    );
}

#[test]
fn replica_equals_a_server_style_fold_in_any_order() {
    for seed in 0..40 {
        let world = World::random(seed + 1_000, 24, 120);
        let creation: Vec<usize> = (0..world.ops.len()).collect();
        let reference = deliver(&world, &creation);
        let mut rng = Rng(seed);
        let order = world.delivery(&mut rng, 25);
        let rows = server_fold(&world, &order);
        let mut seen = 0;
        for page in 1..=4 {
            let page_replica = reference.page(common::page_id(page)).unwrap();
            for state in page_replica.items() {
                seen += 1;
                let row = &rows[&state.item.id];
                assert_eq!(row.stamps, state.stamps, "seed {seed}: stamps");
                assert_eq!(row.deleted, state.deleted, "seed {seed}: tombstone");
                let (merged, _) = row
                    .patch
                    .applied(&row.body, row.deleted)
                    .expect("patch applies");
                assert_eq!(merged, state.item, "seed {seed}: body ⊕ patch");
                // The same row through the wire form.
                let mut body = Vec::new();
                encode_item(&row.body, &mut body);
                let wire = ItemState {
                    page: common::page_id(page),
                    body,
                    patch: (!row.patch.is_empty())
                        .then(|| encode_patch_blob(row.body.id, &row.patch)),
                    stamps: row.stamps.encode(),
                    deleted: row.deleted,
                };
                assert_eq!(
                    &wire.to_item().expect("row decodes"),
                    state,
                    "seed {seed}: ItemState"
                );
            }
        }
        assert_eq!(seen, rows.len());
    }
}

fn snapshot_of(replica: &Replica, pages: &[PageId], chunk_items: usize) -> Vec<SnapshotChunk> {
    let mut chunks = vec![SnapshotChunk {
        notebooks: replica.catalog().notebook_states(),
        pages: replica.catalog().page_states(),
        items: Vec::new(),
    }];
    let rows: Vec<ItemState> = pages
        .iter()
        .flat_map(|&p| {
            replica
                .page(p)
                .unwrap()
                .items()
                .map(move |m| ItemState::from_item(p, m))
        })
        .collect();
    for part in rows.chunks(chunk_items) {
        chunks.push(SnapshotChunk {
            items: part.to_vec(),
            ..SnapshotChunk::default()
        });
    }
    chunks
}

#[test]
fn snapshot_load_equals_the_replica_and_merges_idempotently() {
    let pages: Vec<PageId> = (1..=4).map(common::page_id).collect();
    for seed in 0..20 {
        let world = World::random(seed + 5_000, 24, 100);
        let creation: Vec<usize> = (0..world.ops.len()).collect();
        let source = deliver(&world, &creation);
        let chunks = snapshot_of(&source, &pages, 7);

        let mut loaded = Replica::new();
        loaded.begin_snapshot(Scope::All).unwrap();
        for chunk in &chunks {
            loaded.apply_snapshot_chunk(chunk).unwrap();
        }
        loaded.end_snapshot(Scope::All, 77);
        assert_eq!(loaded.head_seq(), 77);
        let mut expected = source.clone();
        expected.end_snapshot(Scope::All, 77);
        assert!(loaded == expected, "seed {seed}: snapshot load differs");

        // Re-delivering every chunk, and a prefix of live ops beforehand, changes nothing.
        let half: Vec<usize> = (0..world.ops.len() / 2).collect();
        let mut mixed = deliver(&world, &half);
        for chunk in chunks.iter().chain(chunks.iter()) {
            mixed.apply_snapshot_chunk(chunk).unwrap();
        }
        mixed.end_snapshot(Scope::All, 77);
        assert!(
            mixed == expected,
            "seed {seed}: snapshot merge onto live state differs"
        );
    }
}

#[test]
fn page_scoped_snapshot_replaces_only_that_page() {
    let world = World::random(42, 24, 60);
    let creation: Vec<usize> = (0..world.ops.len()).collect();
    let source = deliver(&world, &creation);
    let pages: Vec<PageId> = (1..=4).map(common::page_id).collect();
    let chunks = snapshot_of(&source, &pages, 1000);

    // Browser: catalog first, then one watched page.
    let mut viewer = Replica::new();
    viewer.begin_snapshot(Scope::Catalog).unwrap();
    viewer.apply_snapshot_chunk(&chunks[0]).unwrap();
    assert!(
        viewer
            .page(common::page_id(1))
            .unwrap()
            .items()
            .next()
            .is_none()
    );
    viewer
        .begin_snapshot(Scope::Page(common::page_id(1)))
        .unwrap();
    let only_page_1 = SnapshotChunk {
        items: chunks[1]
            .items
            .iter()
            .filter(|r| r.page == common::page_id(1))
            .cloned()
            .collect(),
        ..SnapshotChunk::default()
    };
    viewer.apply_snapshot_chunk(&only_page_1).unwrap();
    assert_eq!(
        viewer.page(common::page_id(1)).unwrap(),
        source.page(common::page_id(1)).unwrap()
    );
    assert!(
        viewer
            .page(common::page_id(2))
            .unwrap()
            .items()
            .next()
            .is_none()
    );
    assert!(
        viewer
            .begin_snapshot(Scope::Page(common::page_id(99)))
            .is_err()
    );
}
