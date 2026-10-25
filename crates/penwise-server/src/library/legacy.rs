//! Compatibility with the pre-SQLite layout: `meta.json`, `pages/<id>.json` and the older single `page.json`.
//!
//! The database is authoritative; these readers exist so an existing library is imported once and nothing is
//! lost. Nothing in this module writes.

use std::path::Path;

use super::model::{Background, Meta, Notebook, PageKind, PageMeta, StrokeMsg, new_id};

/// A stroke is only storable when its sample and time arrays agree and it has samples.
pub(crate) fn is_well_formed(stroke: &StrokeMsg) -> bool {
    stroke.samples.len() == stroke.times.len() * 5 && !stroke.times.is_empty()
}

/// One page's strokes as the old `pages/<id>.json` stored them.
#[derive(serde::Deserialize)]
struct PageFile {
    #[serde(default)]
    strokes: Vec<StrokeMsg>,
}

/// Portrait sheets from earlier builds become landscape: their ink was recorded in landscape device pixels all
/// along, so a 1600×2560 page both cropped it and drew it rotated relative to the tablet.
fn migrate_orientation(meta: &mut Meta) -> bool {
    let mut changed = false;
    for page in meta
        .notebooks
        .iter_mut()
        .flat_map(|notebook| &mut notebook.pages)
    {
        if let PageKind::Fixed { width, height } = page.kind
            && (width - PORTRAIT_WIDTH).abs() < 0.5
            && (height - PORTRAIT_HEIGHT).abs() < 0.5
        {
            page.kind = PageKind::Fixed {
                width: height,
                height: width,
            };
            changed = true;
        }
    }
    changed
}

/// The pre-landscape default.
const PORTRAIT_WIDTH: f32 = 1600.0;
/// The pre-landscape default.
const PORTRAIT_HEIGHT: f32 = 2560.0;

/// The old index, with the landscape migration applied. `None` when there is no usable `meta.json`.
pub(crate) fn read_meta(dir: &Path) -> Option<Meta> {
    let body = std::fs::read(dir.join("meta.json")).ok()?;
    let mut meta: Meta = serde_json::from_slice(&body).ok()?;
    meta.page(&meta.active)?;
    if migrate_orientation(&mut meta) {
        tracing::info!("migrated pages to landscape sheets");
    }
    Some(meta)
}

/// One old page file's strokes.
pub(crate) fn read_page(dir: &Path, page: &str) -> Vec<StrokeMsg> {
    let path = dir.join("pages").join(format!("{page}.json"));
    let Ok(body) = std::fs::read(&path) else {
        return Vec::new();
    };
    let Ok(file) = serde_json::from_slice::<PageFile>(&body) else {
        tracing::warn!(page, "legacy page unreadable");
        return Vec::new();
    };
    file.strokes.into_iter().filter(is_well_formed).collect()
}

/// The oldest layout: one page in `page.json`.
pub(crate) fn read_single_page(dir: &Path) -> Option<Vec<StrokeMsg>> {
    let body = std::fs::read(dir.join("page.json")).ok()?;
    let page: PageFile = serde_json::from_slice(&body).ok()?;
    Some(page.strokes.into_iter().filter(is_well_formed).collect())
}

/// A first-run library: one notebook holding one landscape sheet.
pub(crate) fn default_meta() -> Meta {
    let page = PageMeta {
        id: new_id("pg"),
        title: "Page 1".to_owned(),
        kind: PageKind::landscape(),
        background: Background::Blank,
    };
    Meta {
        version: 1,
        active: page.id.clone(),
        notebooks: vec![Notebook {
            id: new_id("nb"),
            title: "Notebook 1".to_owned(),
            pages: vec![page],
        }],
    }
}
