//! Column codecs shared by the writer, the readers and the migration.

use penwise_proto::{Hlc, ProtoError};
use rusqlite::{Connection, OptionalExtension, params};

use super::error::StorageError;

/// Parses a 24-byte HLC column.
///
/// # Errors
/// `Corrupt` for any other length.
pub fn parse_hlc(bytes: &[u8]) -> Result<Hlc, StorageError> {
    Hlc::from_bytes(bytes)
        .ok_or_else(|| StorageError::Corrupt(format!("hlc of {} bytes", bytes.len())))
}

/// Maps a proto codec failure on a stored blob to `Corrupt`.
pub fn corrupt(what: &str, error: &ProtoError) -> StorageError {
    StorageError::Corrupt(format!("{what}: {error}"))
}

/// `u64` ↔ `i64` for `INTEGER` columns (seqs and ms never exceed `i64::MAX`).
#[must_use]
pub fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Inverse of [`to_i64`]; negative values (never written) read as 0.
#[must_use]
pub fn to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

/// `server_meta` value.
///
/// # Errors
/// `Sqlite`.
pub fn meta_get(conn: &Connection, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
    Ok(conn
        .query_row("SELECT value FROM server_meta WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

/// `server_meta` integer value (8 bytes little-endian).
///
/// # Errors
/// `Sqlite`, `Corrupt` for a value of the wrong width.
pub fn meta_get_u64(conn: &Connection, key: &str) -> Result<Option<u64>, StorageError> {
    meta_get(conn, key)?
        .map(|bytes| {
            <[u8; 8]>::try_from(bytes.as_slice())
                .map(u64::from_le_bytes)
                .map_err(|_| StorageError::Corrupt(format!("server_meta.{key}")))
        })
        .transpose()
}

/// Upserts a `server_meta` value.
///
/// # Errors
/// `Sqlite`.
pub fn meta_put(conn: &Connection, key: &str, value: &[u8]) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO server_meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Upserts an integer `server_meta` value.
///
/// # Errors
/// `Sqlite`.
pub fn meta_put_u64(conn: &Connection, key: &str, value: u64) -> Result<(), StorageError> {
    meta_put(conn, key, &value.to_le_bytes())
}
