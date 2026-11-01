//! The `generations` table: the v5 → v6 step, recording rows and listing them.

use rusqlite::Connection;

use crate::storage::{Generation, GenerationRole, RoundOp, SCHEMA_VERSION, Storage, StorageConfig};

use super::Rig;

/// The v5 `rounds` table exactly as step 5 shipped it.
const V5_ROUNDS: &str = "
CREATE TABLE rounds (
  project TEXT NOT NULL, n INTEGER NOT NULL, status INTEGER NOT NULL,
  sent_seq INTEGER, resolved_seq INTEGER, sent_ms INTEGER, resolved_ms INTEGER, looked_ms INTEGER,
  PRIMARY KEY (project, n)
) STRICT;
";

fn row(round: u32, role: GenerationRole, label: &str, picked: bool) -> Generation {
    Generation {
        project: "prj_site".to_owned(),
        round,
        route: "/".to_owned(),
        preset: "desktop".to_owned(),
        role,
        label: label.to_owned(),
        picked,
        item: format!("{round}-{role:?}-{label}"),
        asset: "a".repeat(64),
        w: 1440,
        h: 7574,
        resolved_ms: 0,
    }
}

#[test]
fn a_v5_database_gets_the_generations_table_and_keeps_its_rows() {
    let dir = penwise_testkit::TempDir::new("v5-to-v6").expect("tempdir");
    let path = dir.path().join("penwise.db");
    {
        // A v5 database: step 4's tables, step 5's `rounds` with a resolved round, user_version 5.
        let mut conn = Connection::open(&path).expect("open");
        let tx = conn.transaction().expect("tx");
        crate::storage::schema::create_v4(&tx).expect("v4 schema");
        tx.execute_batch(V5_ROUNDS).expect("v5 schema");
        tx.execute(
            "INSERT INTO rounds (project, n, status, resolved_ms) VALUES ('prj_site', 1, 2, 7)",
            [],
        )
        .expect("round");
        tx.pragma_update(None, "user_version", 5).expect("version");
        tx.commit().expect("commit");
        let has: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'generations'",
                [],
                |r| r.get(0),
            )
            .expect("master");
        assert_eq!(has, 0, "a v5 database has no generations table");
    }
    drop(Storage::open(StorageConfig::new(&path)).expect("opens and upgrades"));
    let conn = Connection::open(&path).expect("reopen");
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .expect("version");
    assert_eq!(version, SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 6);
    let columns: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('generations') ORDER BY cid")
        .expect("columns")
        .query_map([], |r| r.get(0))
        .expect("rows")
        .collect::<Result<_, _>>()
        .expect("names");
    assert_eq!(
        columns,
        [
            "project", "round", "route", "preset", "role", "label", "picked", "item", "asset", "w",
            "h"
        ]
    );
    let strips: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('strip_pages') ORDER BY cid")
        .expect("columns")
        .query_map([], |r| r.get(0))
        .expect("rows")
        .collect::<Result<_, _>>()
        .expect("names");
    assert_eq!(strips, ["seq", "project", "route", "preset", "page"]);
    let kept: i64 = conn
        .query_row("SELECT resolved_ms FROM rounds WHERE n = 1", [], |r| {
            r.get(0)
        })
        .expect("round survives");
    assert_eq!(kept, 7);
    drop(Storage::open(StorageConfig::new(&path)).expect("reopens at the current version"));
}

#[tokio::test]
async fn rows_are_listed_newest_round_first_og_before_variants_and_only_for_resolved_rounds() {
    let rig = Rig::new();
    let storage = &rig.storage;
    for n in 1..=3 {
        storage
            .round_op(RoundOp::EnsureOpen {
                project: "prj_site".to_owned(),
            })
            .await
            .unwrap();
        // Rounds 1 and 2 resolve; round 3 stays open, so its rows are not history yet.
        if n < 3 {
            storage
                .round_op(RoundOp::Resolve {
                    project: "prj_site".to_owned(),
                    n,
                })
                .await
                .unwrap()
                .expect("round");
        }
    }
    storage
        .record_generations(vec![
            row(1, GenerationRole::Variant, "B", false),
            row(1, GenerationRole::Og, "", false),
            row(1, GenerationRole::Variant, "A", true),
            row(2, GenerationRole::Og, "", false),
            row(3, GenerationRole::Og, "", false),
        ])
        .await
        .unwrap();
    let all = storage
        .generations("prj_site".to_owned(), None)
        .await
        .unwrap();
    let order: Vec<(u32, GenerationRole, &str)> = all
        .iter()
        .map(|g| (g.round, g.role, g.label.as_str()))
        .collect();
    assert_eq!(
        order,
        [
            (2, GenerationRole::Og, ""),
            (1, GenerationRole::Og, ""),
            (1, GenerationRole::Variant, "A"),
            (1, GenerationRole::Variant, "B"),
        ]
    );
    assert!(all.iter().all(|g| g.resolved_ms > 0), "joined to the round");
    assert!(all.iter().find(|g| g.label == "A").unwrap().picked);
    // The lane filter, and a lane without rows.
    let lane = |route: &str| Some((route.to_owned(), "desktop".to_owned()));
    assert_eq!(
        storage
            .generations("prj_site".to_owned(), lane("/"))
            .await
            .unwrap()
            .len(),
        4
    );
    assert!(
        storage
            .generations("prj_site".to_owned(), lane("/work"))
            .await
            .unwrap()
            .is_empty()
    );
    // A retry replaces the row of the same lane, role and label.
    let mut again = row(2, GenerationRole::Og, "", false);
    again.item = "f".repeat(32);
    storage.record_generations(vec![again]).await.unwrap();
    let all = storage
        .generations("prj_site".to_owned(), None)
        .await
        .unwrap();
    assert_eq!(all.len(), 4, "replaced, not doubled");
    assert_eq!(all[0].item, "f".repeat(32));
}

#[tokio::test]
async fn variant_groups_that_reuse_labels_keep_every_picture_and_recording_a_lane_replaces_it() {
    let rig = Rig::new();
    let storage = &rig.storage;
    storage
        .round_op(RoundOp::EnsureOpen {
            project: "prj_site".to_owned(),
        })
        .await
        .unwrap();
    storage
        .round_op(RoundOp::Resolve {
            project: "prj_site".to_owned(),
            n: 1,
        })
        .await
        .unwrap();
    let picture = |role, label: &str, picked, item: &str| Generation {
        item: format!("{item:0>32}"),
        ..row(1, role, label, picked)
    };
    storage
        .record_generations(vec![
            picture(GenerationRole::Og, "", false, "1"),
            picture(GenerationRole::Variant, "A", true, "2"),
            picture(GenerationRole::Variant, "B", false, "3"),
            picture(GenerationRole::Variant, "A", false, "4"),
            picture(GenerationRole::Variant, "B", true, "5"),
        ])
        .await
        .unwrap();
    let rows = storage
        .generations("prj_site".to_owned(), None)
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        5,
        "two groups of A/B are four variants, not two"
    );
    assert_eq!(
        rows.iter().filter(|g| g.picked).count(),
        2,
        "one pick per group"
    );
    // A retry records the lane again: nothing from the first attempt is left behind.
    storage
        .record_generations(vec![
            picture(GenerationRole::Og, "", false, "9"),
            picture(GenerationRole::Variant, "A", false, "a"),
        ])
        .await
        .unwrap();
    let rows = storage
        .generations("prj_site".to_owned(), None)
        .await
        .unwrap();
    let items: Vec<&str> = rows
        .iter()
        .map(|g| g.item.trim_start_matches('0'))
        .collect();
    assert_eq!(items, ["9", "a"], "the lane was replaced as a whole");
}
