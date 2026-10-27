//! `penwise-server migrate --db <path> [--dry-run]`: lossless v3 → v4 migration of an existing library.

use std::io::Write;
use std::path::PathBuf;

use penwise_server::storage::migrate::{MigrateOptions, migrate};
use penwise_server::storage::system_now_ms;

const USAGE: &str = "usage: penwise-server migrate --db <path> [--dry-run]";

/// Runs the `migrate` subcommand (`args` excludes the subcommand name).
///
/// # Errors
/// Bad arguments, or the migration failed (the v3 database is left untouched).
pub fn run_migrate(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut db: Option<PathBuf> = None;
    let mut dry_run = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--db" => db = Some(iter.next().ok_or(USAGE)?.into()),
            "--dry-run" => dry_run = true,
            _ => return Err(USAGE.into()),
        }
    }
    let db = db.ok_or(USAGE)?;
    let options = MigrateOptions {
        dry_run,
        now_ms: system_now_ms(),
        fault_after_items: None,
    };
    let report = migrate(&db, &options)?;
    write!(std::io::stdout(), "{report}")?;
    Ok(())
}
