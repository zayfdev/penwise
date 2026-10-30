//! `penwise-server migrate`: lossless v3 → v4 migration of a live library (design §3.6).
//!
//! Sequence: integrity check → `VACUUM INTO` backup → one `BEGIN IMMEDIATE` transaction that renames the v3
//! tables to `legacy_*`, creates the v4 schema, replays every notebook/page/object as ops from the server
//! device through the same materialization the live writer uses, verifies, and bumps `user_version`. Any
//! failure rolls everything back, leaving the v3 database untouched. A v4 database is a no-op.

mod build;
mod emit;
mod objects;
mod report;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::error::StorageError;
use super::schema::{self, SCHEMA_VERSION};

pub use report::{MigrationReport, PageCount};

/// Options of one run.
#[derive(Debug, Clone, Default)]
pub struct MigrateOptions {
    /// Migrate a scratch copy (`VACUUM INTO` a temp file) and discard it; the source is only read.
    pub dry_run: bool,
    /// Genesis wall time, Unix ms (the genesis ops' HLC wall).
    pub now_ms: u64,
    /// Fault injection: abort after this many items were inserted (tests the rollback).
    pub fault_after_items: Option<usize>,
}

/// Migrates the database at `path`.
///
/// # Errors
/// `Migration` for a failed integrity check or verification, `Fault` for injected aborts, `Sqlite`, `Io`.
pub fn migrate(path: &Path, options: &MigrateOptions) -> Result<MigrationReport, StorageError> {
    if options.dry_run {
        return dry_run(path, options);
    }
    let mut conn = schema::open_writer(path)?;
    match schema::user_version(&conn)? {
        SCHEMA_VERSION => {
            return Ok(MigrationReport {
                already_current: true,
                ..MigrationReport::default()
            });
        }
        3 => {}
        0 => {
            schema::ensure_current(&mut conn)?;
            return Ok(MigrationReport {
                already_current: true,
                ..MigrationReport::default()
            });
        }
        4 => {
            schema::ensure_current(&mut conn)?;
            return Ok(MigrationReport {
                applied_step: Some(5),
                ..MigrationReport::default()
            });
        }
        v => {
            return Err(StorageError::Migration(format!(
                "cannot migrate schema v{v}"
            )));
        }
    }
    integrity_check(&conn)?;
    let backup = backup_path(path, options.now_ms);
    vacuum_into(&conn, &backup)?;
    let mut report = build::run(&mut conn, options)?;
    report.backup = Some(backup);
    Ok(report)
}

fn dry_run(path: &Path, options: &MigrateOptions) -> Result<MigrationReport, StorageError> {
    let source = schema::open_reader(path)?;
    match schema::user_version(&source)? {
        SCHEMA_VERSION => {
            return Ok(MigrationReport {
                already_current: true,
                dry_run: true,
                ..MigrationReport::default()
            });
        }
        4 => {
            return Ok(MigrationReport {
                pending_step: Some(5),
                dry_run: true,
                ..MigrationReport::default()
            });
        }
        _ => {}
    }
    let scratch = backup_path(path, options.now_ms).with_extension("dryrun");
    vacuum_into(&source, &scratch)?;
    drop(source);
    let result = (|| {
        let mut conn = schema::open_writer(&scratch)?;
        if schema::user_version(&conn)? != 3 {
            return Err(StorageError::Migration("not a schema v3 database".into()));
        }
        integrity_check(&conn)?;
        build::run(&mut conn, options)
    })();
    for suffix in ["", "-wal", "-shm"] {
        let mut name = scratch.clone().into_os_string();
        name.push(suffix);
        let _ = std::fs::remove_file(name);
    }
    result.map(|report| MigrationReport {
        dry_run: true,
        ..report
    })
}

fn integrity_check(conn: &Connection) -> Result<(), StorageError> {
    let verdict: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if verdict == "ok" {
        Ok(())
    } else {
        Err(StorageError::Migration(format!(
            "integrity_check: {verdict}"
        )))
    }
}

fn backup_path(path: &Path, now_ms: u64) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".v3-{now_ms}.bak"));
    PathBuf::from(name)
}

fn vacuum_into(conn: &Connection, target: &Path) -> Result<(), StorageError> {
    if target.exists() {
        return Err(StorageError::Migration(format!(
            "{} already exists",
            target.display()
        )));
    }
    conn.execute("VACUUM INTO ?1", [target.to_string_lossy().as_ref()])?;
    Ok(())
}
