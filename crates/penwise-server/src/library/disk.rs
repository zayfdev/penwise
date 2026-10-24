//! Reading and writing the library on disk: `meta.json`, one file per page, atomic replacement.

use std::path::Path;

use super::model::{Background, Meta, Notebook, PageKind, PageMeta, StrokeMsg, new_id};

pub(crate) fn is_well_formed(stroke: &StrokeMsg) -> bool {
    stroke.samples.len() == stroke.times.len() * 5 && !stroke.times.is_empty()
}

/// Serializes one page exactly as `pages/<id>.json` stores it.
pub(crate) fn page_body(strokes: &[StrokeMsg]) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&PageFile {
        version: 1,
        strokes: strokes.to_vec(),
    })
}

/// One page's strokes as stored in `pages/<id>.json`.
#[derive(serde::Serialize, serde::Deserialize)]
struct PageFile {
    version: u32,
    strokes: Vec<StrokeMsg>,
}

pub(crate) async fn load_or_create_meta(dir: &Path) -> Meta {
    let path = dir.join("meta.json");
    if let Ok(body) = tokio::fs::read(&path).await
        && let Ok(meta) = serde_json::from_slice::<Meta>(&body)
        && meta.page(&meta.active).is_some()
    {
        return meta;
    }
    let meta = migrate_or_fresh(dir).await;
    if let Ok(body) = serde_json::to_vec_pretty(&meta) {
        let _ = tokio::fs::create_dir_all(dir).await;
        let _ = tokio::fs::write(&path, body).await;
    }
    meta
}

/// First run: one notebook with one A4 page, inheriting any legacy `page.json` strokes.
async fn migrate_or_fresh(dir: &Path) -> Meta {
    let page = PageMeta {
        id: new_id("pg"),
        title: "Page 1".to_owned(),
        kind: PageKind::a4(),
        background: Background::Blank,
    };
    let legacy = read_legacy(dir).await;
    let meta = Meta {
        version: 1,
        active: page.id.clone(),
        notebooks: vec![Notebook {
            id: new_id("nb"),
            title: "Notebook 1".to_owned(),
            pages: vec![page.clone()],
        }],
    };
    if let Some(strokes) = legacy {
        let pages_dir = dir.join("pages");
        let _ = tokio::fs::create_dir_all(&pages_dir).await;
        if let Ok(body) = serde_json::to_vec(&PageFile {
            version: 1,
            strokes,
        }) {
            let _ = tokio::fs::write(pages_dir.join(format!("{}.json", page.id)), body).await;
        }
        tracing::info!(page = %page.id, "migrated legacy page.json");
    }
    meta
}

async fn read_legacy(dir: &Path) -> Option<Vec<StrokeMsg>> {
    let body = tokio::fs::read(dir.join("page.json")).await.ok()?;
    let page: PageFile = serde_json::from_slice(&body).ok()?;
    Some(
        page.strokes
            .into_iter()
            .filter(is_well_formed)
            .collect::<Vec<_>>(),
    )
}

pub(crate) async fn read_strokes(dir: &Path, page: &str) -> Result<Vec<StrokeMsg>, String> {
    let path = dir.join("pages").join(format!("{page}.json"));
    match tokio::fs::read(&path).await {
        Ok(body) => {
            let file: PageFile = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
            Ok(file
                .strokes
                .into_iter()
                .filter(is_well_formed)
                .collect::<Vec<_>>())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) async fn write_atomic(path: &Path, body: Vec<u8>, label: &'static str) {
    let path = path.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        write_atomic_blocking(&path, &body).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(())) => tracing::debug!(label, "saved"),
        Ok(Err(error)) => tracing::error!(label, %error, "save failed"),
        Err(error) => tracing::error!(label, %error, "save task failed"),
    }
}

pub(crate) fn write_atomic_blocking(path: &Path, body: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let staging = path.with_extension("json.tmp");
    std::fs::write(&staging, body)?;
    std::fs::rename(&staging, path)
}
