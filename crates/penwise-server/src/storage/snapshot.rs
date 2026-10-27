//! Snapshot rows (`notebooks`, `pages`, `items` as wire types) and the streaming snapshot reader.

use penwise_proto::snapshot::{ItemState, NotebookState, PageState};
use penwise_proto::{Background, NotebookId, PageId, PageKind, Scope, SnapshotChunk};
use rusqlite::{Connection, Row};
use tokio::sync::{mpsc, oneshot};

use super::error::StorageError;
use super::read::head_and_floor;
use super::rows::parse_hlc;
use super::{SNAPSHOT_CHUNK_ITEMS, SnapshotStream, Storage};

/// Soft cap on item bytes per chunk (a frame is ≤ 4 MiB).
const CHUNK_BYTES: usize = 1 << 20;

fn id_of<T>(bytes: &[u8], from: fn(&[u8]) -> Option<T>, what: &str) -> Result<T, StorageError> {
    from(bytes).ok_or_else(|| StorageError::Corrupt(format!("{what} id")))
}

/// Every notebook row.
///
/// # Errors
/// `Corrupt`, `Sqlite`.
pub fn notebook_states(conn: &Connection) -> Result<Vec<NotebookState>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT id, created_hlc, title, title_hlc, position, position_hlc, deleted, deleted_hlc
         FROM notebooks ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
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
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, created, title, title_hlc, position, position_hlc, deleted, deleted_hlc) = row?;
        out.push(NotebookState {
            id: id_of(&id, NotebookId::from_slice, "notebook")?,
            created: parse_hlc(&created)?,
            title,
            title_hlc: parse_hlc(&title_hlc)?,
            position,
            position_hlc: parse_hlc(&position_hlc)?,
            deleted: deleted != 0,
            deleted_hlc: parse_hlc(&deleted_hlc)?,
        });
    }
    Ok(out)
}

const PAGE_COLUMNS: &str = "id, notebook, kind_code, width, height, created_hlc, title, title_hlc,
    position, position_hlc, background, background_hlc, deleted, deleted_hlc";

fn page_state(r: &Row<'_>) -> Result<PageState, StorageError> {
    let id: Vec<u8> = r.get(0)?;
    let notebook: Vec<u8> = r.get(1)?;
    let code: i64 = r.get(2)?;
    let width: Option<f64> = r.get(3)?;
    let height: Option<f64> = r.get(4)?;
    let background: i64 = r.get(10)?;
    let hlc = |i: usize| -> Result<_, StorageError> { parse_hlc(&r.get::<_, Vec<u8>>(i)?) };
    #[allow(clippy::cast_possible_truncation, reason = "stored from f32")]
    let kind = match (code, width, height) {
        (0, Some(w), Some(h)) => PageKind::Fixed {
            width: w as f32,
            height: h as f32,
        },
        _ => PageKind::Infinite,
    };
    Ok(PageState {
        id: id_of(&id, PageId::from_slice, "page")?,
        notebook: id_of(&notebook, NotebookId::from_slice, "notebook")?,
        kind,
        created: hlc(5)?,
        title: r.get(6)?,
        title_hlc: hlc(7)?,
        position: r.get(8)?,
        position_hlc: hlc(9)?,
        background: u8::try_from(background)
            .ok()
            .and_then(Background::from_code)
            .ok_or_else(|| StorageError::Corrupt("page background".into()))?,
        background_hlc: hlc(11)?,
        deleted: r.get::<_, i64>(12)? != 0,
        deleted_hlc: hlc(13)?,
    })
}

/// Every page row (`only` restricts to one id).
///
/// # Errors
/// `Corrupt`, `Sqlite`.
pub fn page_states(
    conn: &Connection,
    only: Option<PageId>,
) -> Result<Vec<PageState>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PAGE_COLUMNS} FROM pages WHERE (?1 IS NULL OR id = ?1) ORDER BY id"
    ))?;
    let mut rows = stmt.query([only.map(|p| p.0.to_vec())])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(page_state(row)?);
    }
    Ok(out)
}

/// The item rows of `page` (all pages for `None`), streamed through `sink` in chunk-sized batches.
///
/// # Errors
/// `Sqlite`, `Corrupt`; the sink's `Err` aborts.
pub fn for_item_batches(
    conn: &Connection,
    page: Option<PageId>,
    mut sink: impl FnMut(Vec<ItemState>) -> Result<(), StorageError>,
) -> Result<(), StorageError> {
    let mut stmt = conn.prepare(
        "SELECT page, body, patch, stamps, deleted FROM items WHERE (?1 IS NULL OR page = ?1)",
    )?;
    let mut rows = stmt.query([page.map(|p| p.0.to_vec())])?;
    let mut batch = Vec::new();
    let mut bytes = 0_usize;
    while let Some(r) = rows.next()? {
        let page: Vec<u8> = r.get(0)?;
        let state = ItemState {
            page: id_of(&page, PageId::from_slice, "item page")?,
            body: r.get(1)?,
            patch: r.get(2)?,
            stamps: r.get(3)?,
            deleted: r.get::<_, i64>(4)? != 0,
        };
        bytes += state.body.len() + state.patch.as_ref().map_or(0, Vec::len);
        batch.push(state);
        if batch.len() >= SNAPSHOT_CHUNK_ITEMS || bytes >= CHUNK_BYTES {
            sink(std::mem::take(&mut batch))?;
            bytes = 0;
        }
    }
    if !batch.is_empty() {
        sink(batch)?;
    }
    Ok(())
}

fn stream(
    conn: &Connection,
    scope: Scope,
    ready: oneshot::Sender<Result<u64, StorageError>>,
    chunks: &mpsc::Sender<Result<SnapshotChunk, StorageError>>,
) -> Result<(), StorageError> {
    let tx = conn.unchecked_transaction()?;
    let head = match head_and_floor(&tx) {
        Ok((head, _)) => head,
        Err(error) => {
            let _ = ready.send(Err(StorageError::Corrupt(error.to_string())));
            return Err(error);
        }
    };
    if ready.send(Ok(head)).is_err() {
        return Ok(());
    }
    let closed = || StorageError::Closed;
    if matches!(scope, Scope::All | Scope::Catalog) {
        let chunk = SnapshotChunk {
            notebooks: notebook_states(&tx)?,
            pages: page_states(&tx, None)?,
            items: Vec::new(),
        };
        chunks.blocking_send(Ok(chunk)).map_err(|_| closed())?;
    }
    let items_of = match scope {
        Scope::All => Some(None),
        Scope::Catalog => None,
        Scope::Page(page) => Some(Some(page)),
    };
    if let Some(page) = items_of {
        for_item_batches(&tx, page, |items| {
            let chunk = SnapshotChunk {
                items,
                ..SnapshotChunk::default()
            };
            chunks.blocking_send(Ok(chunk)).map_err(|_| closed())
        })?;
    }
    Ok(())
}

pub(super) async fn start(storage: &Storage, scope: Scope) -> Result<SnapshotStream, StorageError> {
    let (chunks_tx, chunks) = mpsc::channel(4);
    let (ready_tx, ready_rx) = oneshot::channel();
    let inner = std::sync::Arc::clone(&storage.inner);
    std::thread::Builder::new()
        .name("penwise-snapshot".into())
        .spawn(move || {
            let result = inner
                .readers
                .with(|conn| stream(conn, scope, ready_tx, &chunks_tx));
            if let Err(error) = result
                && !matches!(error, StorageError::Closed)
            {
                let _ = chunks_tx.blocking_send(Err(error));
            }
        })
        .map_err(StorageError::Io)?;
    let head_seq = ready_rx.await.map_err(|_| StorageError::Closed)??;
    Ok(SnapshotStream { head_seq, chunks })
}
