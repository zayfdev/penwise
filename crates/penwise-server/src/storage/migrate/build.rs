//! The v3 → v4 transaction: rename, create, replay as ops, verify, bump.

use penwise_proto::legacy::{legacy_item_id, legacy_notebook_id, legacy_page_id};
use penwise_proto::notebook::MAX_TITLE_BYTES;
use penwise_proto::{
    AnyOp, Background, DeviceId, HlcClock, NotebookChange, NotebookOp, NotebookPatch, PageChange,
    PageKind, PageOp, PagePatch, ServerId,
};
use rusqlite::{Connection, Transaction, TransactionBehavior, params};

use super::emit::{Emitter, count, fail};
use super::objects::migrate_objects;
use super::{MigrateOptions, MigrationReport, PageCount};
use crate::storage::apply;
use crate::storage::error::StorageError;
use crate::storage::random::random_u128;
use crate::storage::rows::{meta_put, meta_put_u64};
use crate::storage::schema::{self, SCHEMA_VERSION};
use crate::storage::writer::seed_meta;

const QUARANTINE_DDL: &str = "CREATE TABLE legacy_unmigrated (
    page TEXT NOT NULL, id TEXT NOT NULL, kind TEXT NOT NULL, codec TEXT NOT NULL,
    payload BLOB NOT NULL, reason TEXT NOT NULL
) STRICT";

const RENAMED: [&str; 4] = ["notebooks", "pages", "objects", "object_history"];

/// Runs the whole migration in one transaction on `conn` (a schema v3 database).
pub(super) fn run(
    conn: &mut Connection,
    options: &MigrateOptions,
) -> Result<MigrationReport, StorageError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut report = MigrationReport {
        notebooks: count(&tx, "notebooks")?,
        pages: count(&tx, "pages")?,
        objects_rows: count(&tx, "objects")?,
        history_rows: count(&tx, "object_history")?,
        anchors_rows: count(&tx, "anchors")?,
        ink_anchor_rows: count(&tx, "ink_anchors")?,
        ..MigrationReport::default()
    };
    for table in RENAMED {
        tx.execute_batch(&format!("ALTER TABLE {table} RENAME TO legacy_{table}"))?;
    }
    schema::create_v4(&tx)?;
    tx.execute_batch(QUARANTINE_DDL)?;
    let server_id = ServerId::from_parts(options.now_ms, random_u128());
    let device = DeviceId::from_parts(options.now_ms, random_u128());
    seed_meta(&tx, server_id, device, options.now_ms)?;
    meta_put_u64(&tx, "migrated_from_v3_ms", options.now_ms)?;
    let mut emitter = Emitter {
        tx: &tx,
        clock: HlcClock::new(device),
        device,
        t0: options.now_ms,
        batch: 1,
        seq: apply::next_seq(&tx)?,
        items: 0,
        fault_after_items: options.fault_after_items,
    };
    migrate_notebooks(&mut emitter, &mut report)?;
    migrate_pages(&mut emitter, &mut report)?;
    report.ops = emitter.seq - 1;
    rewrite_anchors(&tx)?;
    carry_active_page(&tx)?;
    verify(&tx, &report)?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(report)
}

fn sane_title(title: &str, notebook: bool, adjusted: &mut u64) -> String {
    let mut out = title.to_owned();
    if out.len() > MAX_TITLE_BYTES {
        let mut end = MAX_TITLE_BYTES;
        while !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
        *adjusted += 1;
    }
    if notebook && out.trim().is_empty() {
        *adjusted += 1;
        return "Untitled".to_owned();
    }
    out
}

fn migrate_notebooks(
    e: &mut Emitter<'_>,
    report: &mut MigrationReport,
) -> Result<(), StorageError> {
    let rows: Vec<(String, String, i64, bool)> = e
        .tx
        .prepare("SELECT id, title, position, deleted FROM legacy_notebooks ORDER BY position, id")?
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? != 0))
        })?
        .collect::<Result<_, _>>()?;
    for (legacy, title, position, deleted) in rows {
        let notebook = legacy_notebook_id(&legacy);
        let title = sane_title(&title, true, &mut report.adjusted);
        let (op_id, hlc) = (e.op_id(), e.stamp());
        e.emit(&AnyOp::Notebook(NotebookOp {
            op_id,
            device: e.device,
            hlc,
            notebook,
            change: NotebookChange::Create { title, position },
        }))?;
        if deleted {
            let (op_id, hlc) = (e.op_id(), e.stamp());
            e.emit(&AnyOp::Notebook(NotebookOp {
                op_id,
                device: e.device,
                hlc,
                notebook,
                change: NotebookChange::Update(NotebookPatch {
                    deleted: Some(true),
                    ..NotebookPatch::default()
                }),
            }))?;
        }
        alias(e.tx, &legacy, 0, notebook.0)?;
    }
    Ok(())
}

fn alias(tx: &Transaction<'_>, legacy: &str, kind: i64, id: [u8; 16]) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO id_alias (legacy, kind, id) VALUES (?1, ?2, ?3)",
        params![legacy, kind, &id[..]],
    )?;
    Ok(())
}

fn page_kind(text: &str) -> Option<PageKind> {
    if text == "infinite" {
        return Some(PageKind::Infinite);
    }
    let mut parts = text.split(':');
    let (Some("fixed"), Some(w), Some(h), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    let kind = PageKind::Fixed {
        width: w.parse().ok()?,
        height: h.parse().ok()?,
    };
    kind.is_valid().then_some(kind)
}

fn background(text: &str) -> Background {
    match text {
        "ruled" => Background::Lined,
        "grid" => Background::Grid,
        "dots" => Background::Dot,
        _ => Background::Blank,
    }
}

struct LegacyPage {
    id: String,
    notebook: String,
    title: String,
    kind: String,
    background: String,
    position: i64,
    deleted: bool,
}

fn migrate_pages(e: &mut Emitter<'_>, report: &mut MigrationReport) -> Result<(), StorageError> {
    let rows: Vec<LegacyPage> =
        e.tx.prepare(
            "SELECT id, notebook, title, kind, background, position, deleted FROM legacy_pages
             ORDER BY notebook, position, id",
        )?
        .query_map([], |r| {
            Ok(LegacyPage {
                id: r.get(0)?,
                notebook: r.get(1)?,
                title: r.get(2)?,
                kind: r.get(3)?,
                background: r.get(4)?,
                position: r.get(5)?,
                deleted: r.get::<_, i64>(6)? != 0,
            })
        })?
        .collect::<Result<_, _>>()?;
    for page in rows {
        let id = legacy_page_id(&page.id);
        let kind = page_kind(&page.kind).unwrap_or_else(|| {
            report.adjusted += 1;
            PageKind::Infinite
        });
        let title = sane_title(&page.title, false, &mut report.adjusted);
        let (op_id, hlc) = (e.op_id(), e.stamp());
        e.emit(&AnyOp::Page(PageOp {
            op_id,
            device: e.device,
            hlc,
            page: id,
            change: PageChange::Create {
                notebook: legacy_notebook_id(&page.notebook),
                title,
                position: page.position,
                kind,
                background: background(&page.background),
            },
        }))?;
        if page.deleted {
            let (op_id, hlc) = (e.op_id(), e.stamp());
            e.emit(&AnyOp::Page(PageOp {
                op_id,
                device: e.device,
                hlc,
                page: id,
                change: PageChange::Update(PagePatch {
                    deleted: Some(true),
                    ..PagePatch::default()
                }),
            }))?;
        }
        alias(e.tx, &page.id, 1, id.0)?;
        let counts = migrate_objects(e, &page.id, id)?;
        report.strokes += counts.strokes;
        report.images += counts.images;
        report.unmigrated += counts.unmigrated;
        report.per_page.push(PageCount {
            legacy_id: page.id,
            migrated: counts.strokes + counts.images,
            unmigrated: counts.unmigrated,
        });
    }
    Ok(())
}

/// `ink_anchors.object` becomes the item ULID text.
fn rewrite_anchors(tx: &Transaction<'_>) -> Result<(), StorageError> {
    let rows: Vec<(String, String)> = tx
        .prepare("SELECT page, object FROM ink_anchors")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (page, object) in rows {
        let item = legacy_item_id(&page, &object);
        tx.execute(
            "UPDATE ink_anchors SET object = ?3 WHERE page = ?1 AND object = ?2",
            params![page, object, penwise_proto::ItemRef::from(item).to_string()],
        )?;
    }
    Ok(())
}

fn carry_active_page(tx: &Transaction<'_>) -> Result<(), StorageError> {
    let active: Option<String> = tx
        .query_row("SELECT value FROM state WHERE key = 'active'", [], |r| {
            r.get(0)
        })
        .ok();
    if let Some(active) = active {
        let known = tx
            .prepare("SELECT 1 FROM legacy_pages WHERE id = ?1")?
            .exists([&active])?;
        if known {
            meta_put(tx, "current_page", &legacy_page_id(&active).0)?;
        }
    }
    Ok(())
}

fn verify(tx: &Transaction<'_>, report: &MigrationReport) -> Result<(), StorageError> {
    let migrated = report.strokes + report.images;
    if migrated + report.unmigrated != report.objects_rows {
        return Err(fail("migrated + unmigrated != objects rows"));
    }
    if count(tx, "items")? != migrated || count(tx, "legacy_unmigrated")? != report.unmigrated {
        return Err(fail("row counts disagree after the migration"));
    }
    if count(tx, "ops")? != report.ops {
        return Err(fail("ops count disagrees"));
    }
    for page in &report.per_page {
        let id = legacy_page_id(&page.legacy_id);
        let n: i64 = tx.query_row(
            "SELECT COUNT(*) FROM items WHERE page = ?1",
            [&id.0[..]],
            |r| r.get(0),
        )?;
        if u64::try_from(n).unwrap_or(0) != page.migrated {
            return Err(fail(format!(
                "page {} item count disagrees",
                page.legacy_id
            )));
        }
    }
    Ok(())
}
