//! The `rounds` table: the v4 → v5 step, the writer-side changes and the item add seqs.

use penwise_core::edit::Edit;
use rusqlite::Connection;

use super::{Rig, T0};
use crate::storage::{RoundOp, RoundStatus, SCHEMA_VERSION, Storage, StorageConfig};

fn project() -> String {
    "prj_site".to_owned()
}

#[test]
fn a_v4_database_gets_the_rounds_table_and_keeps_its_rows() {
    let dir = penwise_testkit::TempDir::new("v4-to-v5").expect("tempdir");
    let path = dir.path().join("penwise.db");
    {
        // The v4 schema exactly as step 4 shipped it, plus one row of server-local state to carry over.
        let mut conn = Connection::open(&path).expect("open");
        let tx = conn.transaction().expect("tx");
        crate::storage::schema::create_v4(&tx).expect("v4 schema");
        tx.execute(
            "INSERT INTO agent_checkpoints (agent, seq, updated_ms) VALUES ('omp', 41, 1)",
            [],
        )
        .expect("checkpoint");
        tx.pragma_update(None, "user_version", 4).expect("version");
        tx.commit().expect("commit");
        let has_rounds: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'rounds'",
                [],
                |r| r.get(0),
            )
            .expect("master");
        assert_eq!(has_rounds, 0, "a v4 database has no rounds table");
    }
    let storage = Storage::open(StorageConfig::new(&path)).expect("opens and upgrades");
    drop(storage);
    let conn = Connection::open(&path).expect("reopen");
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .expect("version");
    assert_eq!(version, SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 5);
    let columns: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('rounds') ORDER BY cid")
        .expect("columns")
        .query_map([], |r| r.get(0))
        .expect("rows")
        .collect::<Result<_, _>>()
        .expect("names");
    assert_eq!(
        columns,
        [
            "project",
            "n",
            "status",
            "sent_seq",
            "resolved_seq",
            "sent_ms",
            "resolved_ms",
            "looked_ms"
        ]
    );
    let seq: i64 = conn
        .query_row(
            "SELECT seq FROM agent_checkpoints WHERE agent = 'omp'",
            [],
            |r| r.get(0),
        )
        .expect("checkpoint survives");
    assert_eq!(seq, 41);
    // Opening again is a no-op.
    Storage::open(StorageConfig::new(&path)).expect("reopens at v5");
}

#[tokio::test]
#[allow(clippy::too_many_lines, reason = "one scenario, step by step")]
async fn rounds_move_open_sent_resolved_with_the_log_head_as_their_seq() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let first = rig
        .storage
        .round_op(RoundOp::EnsureOpen { project: project() })
        .await
        .unwrap()
        .expect("round");
    assert_eq!((first.n, first.status), (1, RoundStatus::Open));
    let again = rig
        .storage
        .round_op(RoundOp::EnsureOpen { project: project() })
        .await
        .unwrap()
        .expect("round");
    assert_eq!(again.n, 1, "ensuring twice keeps one open round");

    let item = rig.stroke(0);
    rig.one(page, Edit::Add(item)).await;
    let head = rig.storage.status().await.unwrap().head_seq;
    rig.tick(5);
    let sent = rig
        .storage
        .round_op(RoundOp::Send {
            project: project(),
            n: 1,
        })
        .await
        .unwrap()
        .expect("sent");
    assert_eq!(sent.status, RoundStatus::Sent);
    assert_eq!(sent.sent_seq, Some(head));
    assert_eq!(sent.sent_ms, Some(T0 + 5));

    // Round 1 is no longer open: sending it again is a conflict, and sending 2 closes the fresh open round.
    let again = rig
        .storage
        .round_op(RoundOp::Send {
            project: project(),
            n: 1,
        })
        .await
        .unwrap();
    assert!(again.is_none(), "a second send of round 1 is refused");
    let next = rig
        .storage
        .round_op(RoundOp::Send {
            project: project(),
            n: 2,
        })
        .await
        .unwrap()
        .expect("second");
    assert_eq!(next.n, 2, "sending 2 closes the fresh open round");

    let looked = rig
        .storage
        .round_op(RoundOp::Looked {
            project: project(),
            n: 1,
        })
        .await
        .unwrap()
        .expect("looked");
    assert_eq!(looked.looked_ms, Some(T0 + 5));
    let resolved = rig
        .storage
        .round_op(RoundOp::Resolve {
            project: project(),
            n: 1,
        })
        .await
        .unwrap()
        .expect("resolved");
    assert_eq!(resolved.status, RoundStatus::Resolved);
    assert_eq!(resolved.resolved_seq, Some(head));

    let all = rig.storage.rounds(project()).await.unwrap();
    assert_eq!(all.iter().map(|r| r.n).collect::<Vec<_>>(), [1, 2]);
    assert!(
        rig.storage
            .rounds("prj_other".to_owned())
            .await
            .unwrap()
            .is_empty(),
        "rounds are per project"
    );
    let missing = rig
        .storage
        .round_op(RoundOp::Resolve {
            project: project(),
            n: 9,
        })
        .await
        .unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn an_open_round_resolved_directly_is_sent_in_the_same_step() {
    let rig = Rig::new();
    rig.storage
        .round_op(RoundOp::EnsureOpen { project: project() })
        .await
        .unwrap();
    let resolved = rig
        .storage
        .round_op(RoundOp::Resolve {
            project: project(),
            n: 1,
        })
        .await
        .unwrap()
        .expect("resolved");
    assert_eq!(resolved.status, RoundStatus::Resolved);
    assert!(resolved.sent_seq.is_some() && resolved.resolved_seq.is_some());
}

#[tokio::test]
async fn items_remember_the_seq_of_the_op_that_added_them() {
    let mut rig = Rig::new();
    let (_, page) = rig.seed_page().await;
    let first = rig.stroke(1);
    let first_id = first.id;
    rig.one(page, Edit::Add(first)).await;
    let after_first = rig.storage.status().await.unwrap().head_seq;
    let second = rig.stroke(2);
    let second_id = second.id;
    rig.one(page, Edit::Add(second)).await;
    let after_second = rig.storage.status().await.unwrap().head_seq;
    let seqs: std::collections::BTreeMap<_, _> = rig
        .storage
        .item_added_seqs(page)
        .await
        .unwrap()
        .into_iter()
        .collect();
    assert_eq!(seqs[&first_id], after_first);
    assert_eq!(seqs[&second_id], after_second);
}

#[test]
fn a_dry_run_on_a_v4_database_reports_step_5_pending_and_writes_nothing() {
    use crate::storage::migrate::{MigrateOptions, migrate};

    let dir = penwise_testkit::TempDir::new("v4-dry-run").expect("tempdir");
    let path = dir.path().join("penwise.db");
    {
        let mut conn = Connection::open(&path).expect("open");
        let tx = conn.transaction().expect("tx");
        crate::storage::schema::create_v4(&tx).expect("v4 schema");
        tx.pragma_update(None, "user_version", 4).expect("version");
        tx.commit().expect("commit");
    }
    let options = MigrateOptions {
        dry_run: true,
        now_ms: T0,
        fault_after_items: None,
    };
    let report = migrate(&path, &options).expect("dry run");
    assert!(report.dry_run && !report.already_current);
    assert_eq!(report.pending_step, Some(5));
    assert!(report.to_string().contains("step 5 pending"), "{report}");
    let version = |path: &std::path::Path| -> i64 {
        Connection::open(path)
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap()
    };
    assert_eq!(version(&path), 4, "a dry run writes nothing");

    let applied = migrate(
        &path,
        &MigrateOptions {
            dry_run: false,
            ..options
        },
    )
    .expect("migrate");
    assert_eq!(applied.applied_step, Some(5));
    assert_eq!(version(&path), 5);
}
