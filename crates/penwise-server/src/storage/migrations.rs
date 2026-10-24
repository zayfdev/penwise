//! Numbered schema migrations (`rusqlite_migration`, version in `PRAGMA user_version`).
//!
//! Owner: slice a.
//!
//! `MIGRATION_SQL[i]` migrates `user_version` `i → i + 1`. Append only; never edit an applied entry
//! (AGENTS.md "Wire format & storage"). A test calls `migrations().validate()` and checks that v1 creates
//! exactly the tables `server_meta, devices, tokens, ops, notebooks, pages, items`.

use rusqlite::Connection;
use rusqlite_migration::Migrations;

/// Up-migrations in order. v1: `migrations/v1.sql`.
pub const MIGRATION_SQL: &[&str] = &[include_str!("migrations/v1.sql")];

/// Schema version after all migrations (`MIGRATION_SQL.len()`).
pub const SCHEMA_VERSION: usize = 1;

/// `Migrations` over [`MIGRATION_SQL`] (`M::up` each).
#[must_use]
pub fn migrations() -> Migrations<'static> {
    todo!()
}

/// Migrates `conn` to [`SCHEMA_VERSION`] (no-op when current).
///
/// # Errors
/// `rusqlite_migration::Error` (including a database newer than this build).
pub fn migrate(conn: &mut Connection) -> Result<(), rusqlite_migration::Error> {
    todo!()
}
