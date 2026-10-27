//! Loads the server's materialized `notebooks`/`pages`/`items` tables into a replica.

use std::path::Path;

use penwise_proto::{
    Hlc, ItemState, NotebookState, PageId, PageKind, PageState, Replica, Scope, SnapshotChunk,
};
use rusqlite::Connection;

use super::open_ro;

fn hlc(bytes: &[u8]) -> Result<Hlc, String> {
    Hlc::from_bytes(bytes).ok_or_else(|| "bad hlc blob".to_owned())
}

fn id<T>(bytes: &[u8], make: impl Fn(&[u8]) -> Option<T>) -> Result<T, String> {
    make(bytes).ok_or_else(|| "bad id blob".to_owned())
}

/// Loads the materialized `notebooks`/`pages`/`items` tables into a replica (via snapshot rows).
///
/// # Errors
/// `SQLite` or decode error text.
pub fn load_tables(path: &Path) -> Result<Replica, String> {
    let conn = open_ro(path)?;
    let mut chunk = SnapshotChunk::default();
    let mut stmt = conn
        .prepare("SELECT id, created_hlc, title, title_hlc, position, position_hlc, deleted, deleted_hlc FROM notebooks")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Vec<u8>>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Vec<u8>>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, Vec<u8>>(7)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (nid, created, title, title_hlc, position, position_hlc, deleted, deleted_hlc) =
            row.map_err(|e| e.to_string())?;
        chunk.notebooks.push(NotebookState {
            id: id(&nid, penwise_proto::NotebookId::from_slice)?,
            created: hlc(&created)?,
            title,
            title_hlc: hlc(&title_hlc)?,
            position,
            position_hlc: hlc(&position_hlc)?,
            deleted: deleted != 0,
            deleted_hlc: hlc(&deleted_hlc)?,
        });
    }
    drop(stmt);
    load_pages(&conn, &mut chunk)?;
    load_items(&conn, &mut chunk)?;
    let mut replica = Replica::new();
    replica
        .begin_snapshot(Scope::All)
        .map_err(|e| e.to_string())?;
    replica
        .apply_snapshot_chunk(&chunk)
        .map_err(|e| e.to_string())?;
    Ok(replica)
}

fn load_pages(conn: &Connection, chunk: &mut SnapshotChunk) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, notebook, width, height, created_hlc, title, title_hlc, position, position_hlc, \
             background, background_hlc, deleted, deleted_hlc FROM pages",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Option<f64>>(2)?,
                r.get::<_, Option<f64>>(3)?,
                r.get::<_, Vec<u8>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Vec<u8>>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, Vec<u8>>(8)?,
                r.get::<_, i64>(9)?,
                r.get::<_, Vec<u8>>(10)?,
                r.get::<_, i64>(11)?,
                r.get::<_, Vec<u8>>(12)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (
            pid,
            nb,
            w,
            h,
            created,
            title,
            title_hlc,
            position,
            position_hlc,
            bg,
            bg_hlc,
            deleted,
            deleted_hlc,
        ) = row.map_err(|e| e.to_string())?;
        #[allow(
            clippy::cast_possible_truncation,
            reason = "page sides are f32 on the wire"
        )]
        let kind = match (w, h) {
            (Some(width), Some(height)) => PageKind::Fixed {
                width: width as f32,
                height: height as f32,
            },
            _ => PageKind::Infinite,
        };
        chunk.pages.push(PageState {
            id: id(&pid, PageId::from_slice)?,
            notebook: id(&nb, penwise_proto::NotebookId::from_slice)?,
            kind,
            created: hlc(&created)?,
            title,
            title_hlc: hlc(&title_hlc)?,
            position,
            position_hlc: hlc(&position_hlc)?,
            background: u8::try_from(bg)
                .ok()
                .and_then(penwise_proto::Background::from_code)
                .ok_or("bad background code")?,
            background_hlc: hlc(&bg_hlc)?,
            deleted: deleted != 0,
            deleted_hlc: hlc(&deleted_hlc)?,
        });
    }
    Ok(())
}

fn load_items(conn: &Connection, chunk: &mut SnapshotChunk) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT page, body, patch, stamps, deleted FROM items")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Option<Vec<u8>>>(2)?,
                r.get::<_, Vec<u8>>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (page, body, patch, stamps, deleted) = row.map_err(|e| e.to_string())?;
        chunk.items.push(ItemState {
            page: id(&page, PageId::from_slice)?,
            body,
            patch,
            stamps,
            deleted: deleted != 0,
        });
    }
    Ok(())
}
