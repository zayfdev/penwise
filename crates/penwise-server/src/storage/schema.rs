//! Schema v5 and numbered migrations (design §3.1). Every step runs in one transaction; `user_version` is
//! bumped last. A step, once shipped, is never edited: change the schema with a new step.

use rusqlite::{Connection, OpenFlags, Transaction};

use super::error::StorageError;

/// Schema version this build writes and serves.
pub const SCHEMA_VERSION: i64 = 5;

/// Step 4: the op-log schema on an empty database (v3 databases arrive here through [`super::migrate`]).
const V4_DDL: &str = "
CREATE TABLE server_meta (key TEXT PRIMARY KEY, value BLOB NOT NULL) STRICT;
CREATE TABLE devices (
  id BLOB PRIMARY KEY, name TEXT NOT NULL, kind INTEGER NOT NULL,
  created_ms INTEGER NOT NULL, last_seen_ms INTEGER NOT NULL,
  acked_seq INTEGER NOT NULL DEFAULT 0
) STRICT;
CREATE TABLE ops (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  op_id BLOB NOT NULL UNIQUE,
  batch INTEGER NOT NULL,
  device BLOB NOT NULL, hlc BLOB NOT NULL,
  kind INTEGER NOT NULL,
  notebook BLOB, page BLOB, item BLOB,
  body BLOB NOT NULL,
  received_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX ops_page ON ops(page, seq) WHERE page IS NOT NULL;
CREATE INDEX ops_item ON ops(item, seq) WHERE item IS NOT NULL;
CREATE INDEX ops_received ON ops(received_ms);
CREATE TABLE notebooks (
  id BLOB PRIMARY KEY, created_hlc BLOB NOT NULL,
  title TEXT NOT NULL, title_hlc BLOB NOT NULL,
  position INTEGER NOT NULL, position_hlc BLOB NOT NULL,
  deleted INTEGER NOT NULL, deleted_hlc BLOB NOT NULL, updated_seq INTEGER NOT NULL
) STRICT;
CREATE TABLE pages (
  id BLOB PRIMARY KEY, notebook BLOB NOT NULL REFERENCES notebooks(id),
  kind_code INTEGER NOT NULL, width REAL, height REAL,
  created_hlc BLOB NOT NULL,
  title TEXT NOT NULL, title_hlc BLOB NOT NULL,
  position INTEGER NOT NULL, position_hlc BLOB NOT NULL,
  background INTEGER NOT NULL, background_hlc BLOB NOT NULL,
  deleted INTEGER NOT NULL, deleted_hlc BLOB NOT NULL, updated_seq INTEGER NOT NULL
) STRICT;
CREATE INDEX pages_order ON pages(notebook, deleted, position, id);
CREATE TABLE items (
  id BLOB PRIMARY KEY, page BLOB NOT NULL REFERENCES pages(id),
  kind INTEGER NOT NULL,
  layer INTEGER NOT NULL, z INTEGER NOT NULL, deleted INTEGER NOT NULL,
  body BLOB NOT NULL,
  patch BLOB,
  stamps BLOB NOT NULL,
  added_seq INTEGER NOT NULL, updated_seq INTEGER NOT NULL
) STRICT;
CREATE INDEX items_page ON items(page, deleted, z, id);
CREATE INDEX items_layer ON items(page, layer, deleted) WHERE layer <> 0;
CREATE TABLE id_alias (legacy TEXT PRIMARY KEY, kind INTEGER NOT NULL, id BLOB NOT NULL) STRICT;
CREATE TABLE image_labels (item BLOB PRIMARY KEY, label TEXT NOT NULL) STRICT;
";

/// Step 5: design-harness rounds (spec v2 §4). One row per (project, round number); `status` is 0 open, 1 sent,
/// 2 resolved. `sent_seq` / `resolved_seq` are op-log seqs: a mark belongs to the round whose `sent_seq` window
/// holds the seq of the op that added it. Server-local, never synced.
const V5_DDL: &str = "
CREATE TABLE rounds (
  project TEXT NOT NULL,
  n INTEGER NOT NULL,
  status INTEGER NOT NULL,
  sent_seq INTEGER,
  resolved_seq INTEGER,
  sent_ms INTEGER,
  resolved_ms INTEGER,
  looked_ms INTEGER,
  PRIMARY KEY (project, n)
) STRICT;
";

/// Opens the read-write connection with the server pragmas.
///
/// # Errors
/// `Sqlite` when the file cannot be opened or the pragmas fail.
pub fn open_writer(path: &std::path::Path) -> Result<Connection, StorageError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(StorageError::Io)?;
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;
    Ok(conn)
}

/// Opens a read-only WAL connection (the writer must have created the database).
///
/// # Errors
/// `Sqlite` when the file cannot be opened.
pub fn open_reader(path: &std::path::Path) -> Result<Connection, StorageError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_millis(5000))?;
    Ok(conn)
}

/// `PRAGMA user_version`.
///
/// # Errors
/// `Sqlite`.
pub fn user_version(conn: &Connection) -> Result<i64, StorageError> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

/// Ids of items whose tombstone was purged by compaction (with their creation stamp): an `Add` for one of
/// them is a resurrection attempt and is refused as `Duplicate`. Additive within v4 (`IF NOT EXISTS`), created
/// by every open so a v4 database made before the table existed gains it.
const PURGED_DDL: &str = "
CREATE TABLE IF NOT EXISTS purged_items (
  id BLOB PRIMARY KEY, created_hlc BLOB NOT NULL, purged_ms INTEGER NOT NULL
) STRICT;
";

/// Linked projects (design-harness spec §6.1): server-local registry rows, never synced. Additive within v4
/// (`IF NOT EXISTS`), created by every open so a v4 database made before the table existed gains it. `target`
/// is the static folder or the dev origin; `root` is unique so re-linking a folder returns the same project.
const PROJECTS_DDL: &str = "
CREATE TABLE IF NOT EXISTS projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  root TEXT NOT NULL UNIQUE,
  target TEXT NOT NULL,
  port INTEGER NOT NULL,
  notebook TEXT,
  created_ms INTEGER NOT NULL
) STRICT;
";

/// Agent checkpoints (design-harness spec §7.1, C2): the op-log seq each agent last read marks up to.
/// Server-local, never synced. Additive within v4 (`IF NOT EXISTS`), like the project registry.
const AGENT_CHECKPOINTS_DDL: &str = "
CREATE TABLE IF NOT EXISTS agent_checkpoints (
  agent TEXT PRIMARY KEY,
  seq INTEGER NOT NULL,
  updated_ms INTEGER NOT NULL
) STRICT;
";

/// Creates the v4 tables inside `tx` (no version bump).
///
/// # Errors
/// `Sqlite`.
pub fn create_v4(tx: &Transaction<'_>) -> Result<(), StorageError> {
    tx.execute_batch(V4_DDL)?;
    tx.execute_batch(PURGED_DDL)?;
    tx.execute_batch(PROJECTS_DDL)?;
    tx.execute_batch(AGENT_CHECKPOINTS_DDL)?;
    Ok(())
}

/// Creates every table of [`SCHEMA_VERSION`] inside `tx` (steps 4 and 5; no version bump).
///
/// # Errors
/// `Sqlite`.
pub fn create_current(tx: &Transaction<'_>) -> Result<(), StorageError> {
    create_v4(tx)?;
    tx.execute_batch(V5_DDL)?;
    Ok(())
}

/// Step 5 on a v4 database: adds `rounds` and bumps `user_version`, in one transaction.
fn upgrade_v4_to_v5(conn: &mut Connection) -> Result<(), StorageError> {
    let tx = conn.transaction()?;
    tx.execute_batch(V5_DDL)?;
    tx.pragma_update(None, "user_version", 5)?;
    tx.commit()?;
    Ok(())
}

/// Brings a database to [`SCHEMA_VERSION`]: a fresh one gets every step, a v4 one gets step 5. A database at
/// version 1–3 is a v3 library that must go through `penwise-server migrate` first.
///
/// # Errors
/// `NeedsMigration` for v1–3, `FutureSchema` for a newer database, else `Sqlite`.
pub fn ensure_current(conn: &mut Connection) -> Result<(), StorageError> {
    match user_version(conn)? {
        SCHEMA_VERSION => {
            conn.execute_batch(PURGED_DDL)?;
            conn.execute_batch(PROJECTS_DDL)?;
            conn.execute_batch(AGENT_CHECKPOINTS_DDL)?;
            Ok(())
        }
        4 => {
            conn.execute_batch(PURGED_DDL)?;
            conn.execute_batch(PROJECTS_DDL)?;
            conn.execute_batch(AGENT_CHECKPOINTS_DDL)?;
            upgrade_v4_to_v5(conn)
        }
        0 => {
            let tx = conn.transaction()?;
            create_current(&tx)?;
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            tx.commit()?;
            Ok(())
        }
        v @ 1..=3 => Err(StorageError::NeedsMigration { version: v }),
        v => Err(StorageError::FutureSchema { version: v }),
    }
}
