//! Snapshot rows: stored forms, rejection of corrupt rows, catalog row merging.
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

use common::{hlc, item_id, notebook_id, page_id, text_item};
use penwise_core::codec::item::encode_item;
use penwise_core::edit::{Field, ItemPatch};
use penwise_core::item::Affine;
use penwise_proto::snapshot::{decode_patch_blob, encode_patch_blob};
use penwise_proto::{
    Background, FieldStamps, ItemState, MaterializedItem, NotebookState, PageKind, PageState,
    ProtoError, Replica, ReplicaError, Scope, SnapshotChunk,
};

fn materialized() -> MaterializedItem {
    let mut m = MaterializedItem::from_add(text_item(item_id(1), 0, "a"), hlc(1_000, 0, 1));
    m.stamps.set(Field::Layer, hlc(2_000, 0, 2));
    m
}

fn row() -> ItemState {
    ItemState::from_item(page_id(1), &materialized())
}

fn notebook(deleted_at: u64) -> NotebookState {
    NotebookState {
        id: notebook_id(1),
        created: hlc(1_000, 0, 1),
        title: "t".into(),
        title_hlc: hlc(1_000, 0, 1),
        position: 0,
        position_hlc: hlc(1_000, 0, 1),
        deleted: deleted_at > 0,
        deleted_hlc: hlc(deleted_at.max(1_000), 0, 1),
    }
}

fn page(created: u64) -> PageState {
    PageState {
        id: page_id(1),
        notebook: notebook_id(1),
        kind: PageKind::Infinite,
        created: hlc(created, 0, 1),
        title: String::new(),
        title_hlc: hlc(created, 0, 1),
        position: 0,
        position_hlc: hlc(created, 0, 1),
        background: Background::Blank,
        background_hlc: hlc(created, 0, 1),
        deleted: false,
        deleted_hlc: hlc(created, 0, 1),
    }
}

#[test]
fn an_item_row_round_trips_with_its_stamps() {
    let m = materialized();
    assert_eq!(row().to_item().unwrap(), m);
}

#[test]
fn field_stamps_blob_round_trips_and_reads_missing_entries_as_created() {
    let stamps = materialized().stamps;
    assert_eq!(FieldStamps::decode(&stamps.encode()).unwrap(), stamps);
    let short = FieldStamps {
        created: hlc(5, 0, 1),
        fields: vec![hlc(9, 0, 2)],
    };
    assert_eq!(short.get(Field::Deleted), hlc(9, 0, 2));
    assert_eq!(short.get(Field::FontSize), hlc(5, 0, 1));
    let mut grown = short.clone();
    grown.set(Field::Z, hlc(7, 0, 3));
    assert_eq!(
        grown.get(Field::Layer),
        hlc(5, 0, 1),
        "gap filled with created"
    );
    assert_eq!(grown.get(Field::Z), hlc(7, 0, 3));
}

#[test]
fn bad_stamp_blobs_are_rejected() {
    let good = materialized().stamps.encode();
    assert_eq!(FieldStamps::decode(&[]), Err(ProtoError::Empty));
    let mut wrong_version = good.clone();
    wrong_version[0] = 9;
    assert_eq!(
        FieldStamps::decode(&wrong_version),
        Err(ProtoError::UnsupportedVersion(9))
    );
    assert!(matches!(
        FieldStamps::decode(&good[..good.len() - 1]),
        Err(ProtoError::Decode(_))
    ));
    let mut trailing = good.clone();
    trailing.push(0);
    assert_eq!(
        FieldStamps::decode(&trailing),
        Err(ProtoError::TrailingBytes { count: 1 })
    );
    let too_many = FieldStamps {
        created: hlc(1, 0, 1),
        fields: vec![hlc(1, 0, 1); Field::ALL.len() + 1],
    };
    assert_eq!(
        FieldStamps::decode(&too_many.encode()),
        Err(ProtoError::Invalid("field stamps"))
    );
}

#[test]
fn corrupt_item_rows_are_rejected() {
    let mut bad_body = row();
    bad_body.body[10] ^= 0xFF;
    assert!(matches!(bad_body.to_item(), Err(ProtoError::Codec(_))));

    let mut bad_stamps = row();
    bad_stamps.stamps = vec![1, 0xFF];
    assert!(bad_stamps.to_item().is_err());

    let patch = ItemPatch {
        transform: Some(Affine::translate(3.0, 4.0)),
        ..ItemPatch::default()
    };
    let mut ok = row();
    ok.patch = Some(encode_patch_blob(item_id(1), &patch));
    assert_eq!(
        ok.to_item().unwrap().item.transform,
        Affine::translate(3.0, 4.0),
        "body ⊕ patch"
    );

    let mut other_item = row();
    other_item.patch = Some(encode_patch_blob(item_id(2), &patch));
    assert_eq!(
        other_item.to_item(),
        Err(ProtoError::Invalid("item patch blob"))
    );

    let sets_deleted = ItemPatch {
        deleted: Some(true),
        ..ItemPatch::default()
    };
    let mut tombstone = row();
    tombstone.patch = Some(encode_patch_blob(item_id(1), &sets_deleted));
    assert_eq!(
        tombstone.to_item(),
        Err(ProtoError::Invalid("item patch sets deleted"))
    );

    let wrong_field = ItemPatch {
        shape_size: Some([1.0, 1.0]),
        ..ItemPatch::default()
    };
    let mut inapplicable = row();
    inapplicable.patch = Some(encode_patch_blob(item_id(1), &wrong_field));
    assert_eq!(
        inapplicable.to_item(),
        Err(ProtoError::Invalid("item patch"))
    );

    // A patch blob that is an Add (not an Update) is not a patch.
    let mut body = Vec::new();
    encode_item(&text_item(item_id(1), 0, "a"), &mut body);
    assert!(decode_patch_blob(&body, item_id(1)).is_err());
}

#[test]
fn snapshot_rows_enforce_create_before_reference() {
    let mut replica = Replica::new();
    replica.begin_snapshot(Scope::All).unwrap();
    // Page row before its notebook.
    let orphan_page = SnapshotChunk {
        pages: vec![page(1_000)],
        ..SnapshotChunk::default()
    };
    assert_eq!(
        replica.apply_snapshot_chunk(&orphan_page),
        Err(ReplicaError::UnknownNotebook)
    );
    // Item row before its page.
    let orphan_item = SnapshotChunk {
        items: vec![row()],
        ..SnapshotChunk::default()
    };
    assert_eq!(
        replica.apply_snapshot_chunk(&orphan_item),
        Err(ReplicaError::UnknownPage)
    );
    // A corrupt item row aborts the chunk with a typed error.
    let mut corrupt = row();
    corrupt.body.clear();
    let chunk = SnapshotChunk {
        notebooks: vec![notebook(0)],
        pages: vec![page(1_000)],
        items: vec![corrupt],
    };
    assert!(matches!(
        replica.apply_snapshot_chunk(&chunk),
        Err(ReplicaError::Snapshot(_))
    ));
}

#[test]
fn catalog_rows_merge_by_register_and_refuse_a_different_creation() {
    let mut replica = Replica::new();
    let chunk = |nb: NotebookState, pg: PageState| SnapshotChunk {
        notebooks: vec![nb],
        pages: vec![pg],
        items: Vec::new(),
    };
    let first = replica
        .apply_snapshot_chunk(&chunk(notebook(0), page(1_000)))
        .unwrap();
    assert!(first.catalog_changed);
    // Same rows again: no change.
    let again = replica
        .apply_snapshot_chunk(&chunk(notebook(0), page(1_000)))
        .unwrap();
    assert!(!again.catalog_changed);
    // A newer tombstone register wins; an older one is ignored.
    let deleted = replica
        .apply_snapshot_chunk(&chunk(notebook(5_000), page(1_000)))
        .unwrap();
    assert!(deleted.catalog_changed);
    assert!(replica.catalog().notebook(notebook_id(1)).unwrap().deleted);
    let older = NotebookState {
        deleted: false,
        deleted_hlc: hlc(3_000, 0, 1),
        ..notebook(0)
    };
    replica
        .apply_snapshot_chunk(&chunk(older, page(1_000)))
        .unwrap();
    assert!(replica.catalog().notebook(notebook_id(1)).unwrap().deleted);
    // Another creation stamp for the same id is a conflict, never merged.
    assert_eq!(
        replica.apply_snapshot_chunk(&chunk(notebook(0), page(2_000))),
        Err(ReplicaError::Duplicate)
    );
}

#[test]
fn begin_snapshot_resets_exactly_what_the_scope_covers() {
    let mut replica = Replica::new();
    let chunk = SnapshotChunk {
        notebooks: vec![notebook(0)],
        pages: vec![page(1_000)],
        items: vec![row()],
    };
    replica.apply_snapshot_chunk(&chunk).unwrap();
    replica.end_snapshot(9);
    assert_eq!(replica.page(page_id(1)).unwrap().items().count(), 1);

    replica.begin_snapshot(Scope::Page(page_id(1))).unwrap();
    assert_eq!(replica.page(page_id(1)).unwrap().items().count(), 0);
    assert!(replica.catalog().page(page_id(1)).is_some());
    assert_eq!(replica.head_seq(), 9, "a page snapshot keeps the cursor");

    replica.begin_snapshot(Scope::Catalog).unwrap();
    assert!(replica.catalog().notebook(notebook_id(1)).is_none());
    assert!(replica.page(page_id(1)).is_none());

    replica.apply_snapshot_chunk(&chunk).unwrap();
    replica.begin_snapshot(Scope::All).unwrap();
    assert_eq!(replica, Replica::new());
}
