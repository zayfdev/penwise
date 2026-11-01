//! v3 → v4 migration over the generated historical database.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test fixtures: small counts and coordinates"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use penwise_core::codec::item::decode_item;
use penwise_core::item::{ItemKind, Layer};
use penwise_proto::Replica;
use penwise_proto::legacy::{legacy_item_id, legacy_page_id, legacy_seed};
use penwise_testkit::TempDir;
use penwise_testkit::fixture::{DbFixture, Expected};
use rusqlite::Connection;

use super::{MigrateOptions, migrate};
use crate::storage::{AliasTarget, Storage, StorageConfig, StorageError};

const NOW: u64 = 1_800_000_000_000;

fn options() -> MigrateOptions {
    MigrateOptions {
        dry_run: false,
        now_ms: NOW,
        fault_after_items: None,
    }
}

fn fixture(dir: &TempDir, name: &str) -> (PathBuf, Expected) {
    let path = dir.path().join(name);
    let expected = DbFixture::build(&path).expect("fixture");
    (path, expected)
}

fn scalar(path: &Path, sql: &str) -> i64 {
    Connection::open(path)
        .expect("open")
        .query_row(sql, [], |r| r.get(0))
        .expect("scalar")
}

fn item_ids(path: &Path) -> BTreeSet<Vec<u8>> {
    let conn = Connection::open(path).expect("open");
    let mut stmt = conn.prepare("SELECT id FROM items").expect("prepare");
    stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))
        .expect("query")
        .map(|r| r.expect("row"))
        .collect()
}

#[test]
fn every_historical_shape_migrates_with_counts_preserved() {
    let dir = TempDir::new("migrate").expect("tempdir");
    let (path, expected) = fixture(&dir, "penwise.db");
    let report = migrate(&path, &options()).expect("migrate");

    assert_eq!(report.objects_rows as usize, expected.objects_rows);
    assert_eq!(
        report.strokes + report.images + report.unmigrated,
        report.objects_rows
    );
    assert_eq!(expected.accounted_rows(), expected.objects_rows);
    for page in &expected.pages {
        let found = report
            .per_page
            .iter()
            .find(|p| p.legacy_id == page.id)
            .expect("page reported");
        assert_eq!(
            (found.migrated as usize, found.unmigrated as usize),
            (page.migrated, page.unmigrated)
        );
        let id = legacy_page_id(&page.id);
        let conn = Connection::open(&path).expect("open");
        let items: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM items WHERE page = ?1",
                [&id.0[..]],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(items as usize, page.migrated, "page {}", page.id);
    }
    assert_eq!(scalar(&path, "PRAGMA user_version"), 6);
    assert_eq!(
        scalar(&path, "SELECT COUNT(*) FROM legacy_unmigrated") as u64,
        report.unmigrated
    );
    assert_eq!(scalar(&path, "SELECT COUNT(*) FROM ops") as u64, report.ops);
    // Legacy tables are kept untouched.
    assert_eq!(
        scalar(&path, "SELECT COUNT(*) FROM legacy_objects") as usize,
        expected.objects_rows
    );
    assert_eq!(
        scalar(&path, "SELECT COUNT(*) FROM legacy_object_history") as usize,
        expected.history_rows
    );
    assert_eq!(
        scalar(&path, "SELECT COUNT(*) FROM legacy_notebooks") as usize,
        expected.notebooks.len()
    );
    assert_eq!(
        scalar(&path, "SELECT COUNT(*) FROM legacy_pages") as usize,
        expected.pages.len()
    );
    assert_eq!(
        scalar(&path, "SELECT COUNT(*) FROM anchors") as usize,
        expected.anchors_rows
    );
    assert_eq!(
        scalar(&path, "SELECT COUNT(*) FROM ink_anchors") as usize,
        expected.ink_anchor_rows
    );
    // Backup exists and is the untouched v3 database.
    let backup = report.backup.expect("backup");
    assert_eq!(scalar(&backup, "PRAGMA user_version"), 3);
    assert_eq!(
        scalar(&backup, "SELECT COUNT(*) FROM objects") as usize,
        expected.objects_rows
    );
}

#[test]
fn strokes_keep_seed_argb_z_layer_and_images_keep_labels() {
    let dir = TempDir::new("migrate").expect("tempdir");
    let (path, expected) = fixture(&dir, "penwise.db");
    migrate(&path, &options()).expect("migrate");
    let conn = Connection::open(&path).expect("open");
    for want in &expected.strokes {
        let id = legacy_item_id(&want.page, &want.row_id);
        let (body, layer, z): (Vec<u8>, i64, i64) = conn
            .query_row(
                "SELECT body, layer, z FROM items WHERE id = ?1",
                [&id.0[..]],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("stroke row");
        let item = decode_item(&body).expect("decode");
        let ItemKind::Stroke(stroke) = item.kind else {
            panic!("not a stroke")
        };
        assert_eq!(
            stroke.seed,
            want.seed.unwrap_or_else(|| legacy_seed(&want.row_id)),
            "{}",
            want.row_id
        );
        assert_eq!(stroke.params.color.to_argb(), want.argb.cast_unsigned());
        assert_eq!((z, stroke.samples.len()), (want.z, want.samples));
        assert_eq!(layer, i64::from(want.agent));
        assert_eq!(
            item.layer,
            if want.agent { Layer::Agent } else { Layer::Ink }
        );
    }
    for want in &expected.images {
        let id = legacy_item_id(&want.page, &want.row_id);
        let z: i64 = conn
            .query_row("SELECT z FROM items WHERE id = ?1", [&id.0[..]], |r| {
                r.get(0)
            })
            .expect("image row");
        assert_eq!(z, want.z);
        let label: Option<String> = conn
            .query_row(
                "SELECT label FROM image_labels WHERE item = ?1",
                [&id.0[..]],
                |r| r.get(0),
            )
            .ok();
        assert_eq!(label, want.label);
    }
    // The active page became the server's current page; aliases resolve; ink anchors point at item ULIDs.
    let active = legacy_page_id(&expected.active_page);
    let object: String = conn
        .query_row("SELECT object FROM ink_anchors LIMIT 1", [], |r| r.get(0))
        .expect("anchor");
    assert_eq!(object.len(), 26);
    drop(conn);
    let storage = Storage::open(StorageConfig::new(&path)).expect("open v4");
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("rt");
    rt.block_on(async {
        assert_eq!(storage.current_page().await.expect("page"), Some(active));
        let alias = storage
            .resolve_alias(expected.active_page.clone())
            .await
            .expect("alias");
        assert_eq!(alias, Some(AliasTarget::Page(active)));
    });
}

#[test]
fn ids_are_deterministic_and_a_rerun_is_a_no_op() {
    let dir = TempDir::new("migrate").expect("tempdir");
    let (first, expected) = fixture(&dir, "one.db");
    let (second, _) = fixture(&dir, "two.db");
    let a = migrate(&first, &options()).expect("migrate");
    let b = migrate(
        &second,
        &MigrateOptions {
            now_ms: NOW + 5,
            ..options()
        },
    )
    .expect("migrate");
    assert_eq!(item_ids(&first), item_ids(&second));
    assert_eq!(item_ids(&first).len() as u64, a.strokes + a.images);
    assert_eq!(a.ops, b.ops);
    let want = legacy_item_id(&expected.strokes[0].page, &expected.strokes[0].row_id);
    assert!(item_ids(&first).contains(want.0.as_slice()));

    let before = scalar(&first, "SELECT COUNT(*) FROM ops");
    let again = migrate(&first, &options()).expect("rerun");
    assert!(again.already_current);
    assert_eq!(scalar(&first, "SELECT COUNT(*) FROM ops"), before);
    assert!(again.backup.is_none());
}

#[test]
fn a_fault_after_some_inserts_leaves_the_v3_database_untouched() {
    let dir = TempDir::new("migrate").expect("tempdir");
    let (path, expected) = fixture(&dir, "penwise.db");
    let error = migrate(
        &path,
        &MigrateOptions {
            fault_after_items: Some(3),
            ..options()
        },
    )
    .expect_err("fault");
    assert!(matches!(error, StorageError::Fault));
    assert_eq!(scalar(&path, "PRAGMA user_version"), 3);
    assert_eq!(
        scalar(&path, "SELECT COUNT(*) FROM objects") as usize,
        expected.objects_rows
    );
    assert_eq!(
        scalar(&path, "SELECT COUNT(*) FROM notebooks") as usize,
        expected.notebooks.len()
    );
    let conn = Connection::open(&path).expect("open");
    let leftovers: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('ops', 'items', 'legacy_objects', 'server_meta')",
            [],
            |r| r.get(0),
        )
        .expect("master");
    assert_eq!(leftovers, 0);
    // The failed run left a valid backup; a retry works.
    migrate(
        &path,
        &MigrateOptions {
            now_ms: NOW + 1,
            ..options()
        },
    )
    .expect("retry");
    assert_eq!(scalar(&path, "PRAGMA user_version"), 6);
}

#[test]
fn dry_run_reports_the_same_counts_and_writes_nothing() {
    let dir = TempDir::new("migrate").expect("tempdir");
    let (path, expected) = fixture(&dir, "penwise.db");
    let before: Vec<_> = std::fs::read_dir(dir.path())
        .expect("dir")
        .map(|e| e.expect("e").file_name())
        .collect();
    let dry = migrate(
        &path,
        &MigrateOptions {
            dry_run: true,
            ..options()
        },
    )
    .expect("dry");
    assert!(dry.dry_run && dry.backup.is_none());
    assert_eq!(dry.objects_rows as usize, expected.objects_rows);
    assert_eq!(scalar(&path, "PRAGMA user_version"), 3);
    let after: Vec<_> = std::fs::read_dir(dir.path())
        .expect("dir")
        .map(|e| e.expect("e").file_name())
        .collect();
    assert_eq!(before.len(), after.len());
    let real = migrate(&path, &options()).expect("migrate");
    assert_eq!(
        (dry.strokes, dry.images, dry.unmigrated, dry.ops),
        (real.strokes, real.images, real.unmigrated, real.ops)
    );
    assert!(dry.to_string().contains("dry run"));
}

#[tokio::test]
async fn a_v3_file_is_refused_by_storage_and_the_migrated_log_folds_to_the_rows() {
    let dir = TempDir::new("migrate").expect("tempdir");
    let (path, _) = fixture(&dir, "penwise.db");
    assert!(matches!(
        Storage::open(StorageConfig::new(&path)),
        Err(StorageError::NeedsMigration { version: 3 })
    ));
    migrate(&path, &options()).expect("migrate");
    let storage = Storage::open(StorageConfig::new(&path)).expect("open");
    let log = storage
        .ops_since(0, crate::storage::OpFilter::All, 100_000)
        .await
        .expect("log");
    let mut replica = Replica::new();
    for op in &log.ops {
        replica.apply(&op.op).expect("genesis op applies");
    }
    assert_eq!(
        *replica.catalog(),
        storage.catalog().await.expect("catalog")
    );
    let mut checked = 0;
    for state in storage.catalog().await.expect("catalog").page_states() {
        let view = storage
            .page_view(state.id)
            .await
            .expect("view")
            .expect("page");
        let folded: Vec<_> = replica
            .page(state.id)
            .expect("replica page")
            .items()
            .cloned()
            .collect();
        assert_eq!(view.replica.items().cloned().collect::<Vec<_>>(), folded);
        checked += folded.len();
    }
    assert!(checked > 0);
}

#[test]
fn the_server_clock_resumes_above_every_genesis_stamp() {
    let dir = TempDir::new("migrate").expect("tempdir");
    let (path, _) = fixture(&dir, "penwise.db");
    migrate(&path, &options()).expect("migrate");
    let conn = Connection::open(&path).expect("open");
    let stored: Vec<u8> = conn
        .query_row(
            "SELECT value FROM server_meta WHERE key = 'clock_hlc'",
            [],
            |r| r.get(0),
        )
        .expect("clock_hlc is stored by the migration");
    let clock = crate::storage::rows::parse_hlc(&stored).expect("hlc");
    let mut stmt = conn.prepare("SELECT hlc FROM ops").expect("prepare");
    let newest = stmt
        .query_map([], |r| r.get::<_, Vec<u8>>(0))
        .expect("query")
        .map(|r| crate::storage::rows::parse_hlc(&r.expect("row")).expect("hlc"))
        .max()
        .expect("ops");
    assert_eq!(
        clock, newest,
        "the persisted clock is the last genesis stamp"
    );
}

#[test]
fn silent_repairs_are_counted_as_adjusted() {
    let dir = TempDir::new("migrate").expect("tempdir");
    let (clean, _) = fixture(&dir, "clean.db");
    let (path, expected) = fixture(&dir, "repaired.db");
    let page = expected.pages[0].id.clone();
    {
        let conn = Connection::open(&path).expect("open");
        let samples = [10.0_f32, 10.0, 0.5, 0.1, 0.0, 20.0, 20.0, 0.5, 0.1, 0.0];
        // A negative time, a preset past the last one and a scale that is not positive: three repairs of
        // one stroke are one adjusted stroke; a second, clean stroke adds none.
        let repaired =
            penwise_testkit::fixture::stroke_blob(9_999, -1, -2.0, 11, &samples, &[-5, 7]);
        let clean_blob = penwise_testkit::fixture::stroke_blob(1, -1, 1.0, 12, &samples, &[0, 7]);
        for (id, blob) in [("rep-1", repaired), ("rep-2", clean_blob)] {
            conn.execute(
                "INSERT INTO objects (page, id, kind, layer, z, codec, payload)
                 VALUES (?1, ?2, 'stroke', 'ink', 900, 'stroke_v1', ?3)",
                rusqlite::params![page, id, blob],
            )
            .expect("insert");
        }
    }
    let base = migrate(&clean, &options()).expect("clean");
    let report = migrate(&path, &options()).expect("repaired");
    assert_eq!(report.strokes, base.strokes + 2, "both strokes migrate");
    assert_eq!(
        report.adjusted,
        base.adjusted + 1,
        "only the repaired stroke counts"
    );
}
