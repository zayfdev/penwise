//! The element maps in `page_anchors`, keyed by the page's id (not synced; see [`crate::sidecar`]).
//!
//! A library migrated from the pre-rewrite server still has its old `anchors` table, keyed by the legacy page id
//! text; the first read or write of a page copies that page's rows across (once, remembered in
//! `page_anchors_seen`). The old `ink_anchors` table is not carried over: which items belong to which element is
//! derived from the page's items each time it is asked for, so it cannot go stale.

use penwise_proto::PageId;
use rusqlite::{Connection, OptionalExtension, params};

use super::Anchor;

const DDL: &str = "
CREATE TABLE IF NOT EXISTS page_anchors (
  page BLOB NOT NULL, selector TEXT NOT NULL, rect TEXT NOT NULL,
  role TEXT, text TEXT, url TEXT,
  PRIMARY KEY (page, selector)
) STRICT;
CREATE TABLE IF NOT EXISTS page_anchors_seen (page BLOB PRIMARY KEY) STRICT;
";

fn prepare(conn: &Connection, page: PageId) -> rusqlite::Result<()> {
    conn.execute_batch(DDL)?;
    let seen = conn
        .query_row(
            "SELECT 1 FROM page_anchors_seen WHERE page = ?1",
            [&page.0[..]],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if seen {
        return Ok(());
    }
    let legacy_table = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'anchors'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if legacy_table {
        conn.execute(
            "INSERT OR IGNORE INTO page_anchors (page, selector, rect, role, text, url)
             SELECT ?1, a.selector, a.rect, a.role, a.text, a.url
             FROM anchors a JOIN id_alias m ON m.legacy = a.page
             WHERE m.kind = 1 AND m.id = ?1",
            [&page.0[..]],
        )?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO page_anchors_seen (page) VALUES (?1)",
        [&page.0[..]],
    )?;
    Ok(())
}

/// The page's element map, ordered by selector.
pub(super) fn read(conn: &Connection, page: PageId) -> rusqlite::Result<Vec<Anchor>> {
    prepare(conn, page)?;
    let mut stmt = conn.prepare(
        "SELECT selector, rect, role, text, url FROM page_anchors WHERE page = ?1 ORDER BY selector",
    )?;
    let rows = stmt.query_map([&page.0[..]], |row| {
        let rect: String = row.get(1)?;
        Ok(Anchor {
            selector: row.get(0)?,
            rect: serde_json::from_str(&rect).unwrap_or([0.0; 4]),
            role: row.get(2)?,
            text: row.get(3)?,
            url: row.get(4)?,
        })
    })?;
    rows.collect()
}

/// Replaces the page's element map.
pub(super) fn replace(conn: &Connection, page: PageId, anchors: &[Anchor]) -> rusqlite::Result<()> {
    prepare(conn, page)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM page_anchors WHERE page = ?1", [&page.0[..]])?;
    for anchor in anchors {
        tx.execute(
            "INSERT OR REPLACE INTO page_anchors (page, selector, rect, role, text, url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &page.0[..],
                anchor.selector,
                serde_json::to_string(&anchor.rect).unwrap_or_else(|_| "[0,0,0,0]".to_owned()),
                anchor.role,
                anchor.text,
                anchor.url
            ],
        )?;
    }
    tx.commit()
}
