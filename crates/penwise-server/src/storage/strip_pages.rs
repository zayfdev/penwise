//! Strip pages (locked-frames contract §2.3): which sketch pages hang off which screen's strip in the overview.
//! Server-local, never synced.

use penwise_proto::PageId;
use rusqlite::{Connection, params};

use super::error::StorageError;

/// A page attached to the strip of a screen (`route` + `preset`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripPage {
    pub page: PageId,
    pub route: String,
    pub preset: String,
}

/// Attaches `row.page` to a strip of `project`; attaching it again to the same strip changes nothing, and to
/// another strip moves it there (it keeps its place in the attach order).
pub(crate) fn attach(
    conn: &Connection,
    project: &str,
    row: &StripPage,
) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO strip_pages (project, route, preset, page) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(project, page) DO UPDATE SET route = excluded.route, preset = excluded.preset",
        params![project, row.route, row.preset, &row.page.0[..]],
    )?;
    Ok(())
}

/// The strip pages of `project` in attach order, without pages deleted from the library.
pub(crate) fn list(conn: &Connection, project: &str) -> Result<Vec<StripPage>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT s.page, s.route, s.preset FROM strip_pages s
         JOIN pages p ON p.id = s.page AND p.deleted = 0
         WHERE s.project = ?1 ORDER BY s.seq",
    )?;
    let mut rows = stmt.query([project])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let id: [u8; 16] = row.get(0)?;
        out.push(StripPage {
            page: PageId(id),
            route: row.get(1)?,
            preset: row.get(2)?,
        });
    }
    Ok(out)
}
