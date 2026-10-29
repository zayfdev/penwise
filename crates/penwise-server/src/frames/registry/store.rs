//! The `projects` table (`sidecar`): registry rows live beside the op log, never sync, and survive restarts.
//!
//! Each call opens its own short-lived connection on the blocking pool, like picture labels, so registry writes
//! never touch the storage writer thread: they carry no ops and nothing waits on them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::Project;
use crate::api::ApiError;
use crate::frames::Target;

const COLUMNS: &str = "id, name, root, target, port, notebook";

/// Every row, in link order.
pub(super) async fn list(data_dir: &Arc<PathBuf>) -> Result<Vec<Project>, ApiError> {
    let data_dir = Arc::clone(data_dir);
    crate::sidecar::with_connection(&data_dir, |conn| {
        let mut statement = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM projects ORDER BY created_ms, id"
        ))?;
        let rows = statement.query_map([], project_of_row)?;
        rows.collect()
    })
    .await
}

/// One row.
pub(super) async fn get(data_dir: &Arc<PathBuf>, id: &str) -> Result<Option<Project>, ApiError> {
    let (data_dir, id) = (Arc::clone(data_dir), id.to_owned());
    crate::sidecar::with_connection(&data_dir, move |conn| {
        let mut statement =
            conn.prepare(&format!("SELECT {COLUMNS} FROM projects WHERE id = ?1"))?;
        let mut rows = statement.query_map([id], project_of_row)?;
        rows.next().transpose()
    })
    .await
}

/// Adds `project`; `root` is unique, so linking the same folder twice is a conflict at this level only after the
/// registry has resolved it to an update.
pub(super) async fn insert(data_dir: &Arc<PathBuf>, project: &Project) -> Result<(), ApiError> {
    let (data_dir, project, created) = (Arc::clone(data_dir), project.clone(), wall_ms());
    crate::sidecar::with_connection(&data_dir, move |conn| {
        conn.execute(
            "INSERT INTO projects (id, name, root, target, port, notebook, created_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                project.id,
                project.name,
                path_text(&project.root),
                store_target(&project.target),
                i64::from(project.port),
                project.notebook,
                created,
            ],
        )
        .map(drop)
    })
    .await
}

/// Overwrites the row of `project.id`.
pub(super) async fn update(data_dir: &Arc<PathBuf>, project: &Project) -> Result<(), ApiError> {
    let (data_dir, project) = (Arc::clone(data_dir), project.clone());
    crate::sidecar::with_connection(&data_dir, move |conn| {
        conn.execute(
            "UPDATE projects
             SET name = ?2, root = ?3, target = ?4, port = ?5, notebook = ?6
             WHERE id = ?1",
            rusqlite::params![
                project.id,
                project.name,
                path_text(&project.root),
                store_target(&project.target),
                i64::from(project.port),
                project.notebook,
            ],
        )
        .map(drop)
    })
    .await
}

/// Removes the row of `id`.
pub(super) async fn delete(data_dir: &Arc<PathBuf>, id: &str) -> Result<(), ApiError> {
    let (data_dir, id) = (Arc::clone(data_dir), id.to_owned());
    crate::sidecar::with_connection(&data_dir, move |conn| {
        conn.execute("DELETE FROM projects WHERE id = ?1", [id])
            .map(drop)
    })
    .await
}

/// Records the notebook created for `id`.
pub(super) async fn set_notebook(
    data_dir: &Arc<PathBuf>,
    id: &str,
    notebook: &str,
) -> Result<(), ApiError> {
    let (data_dir, id, notebook) = (Arc::clone(data_dir), id.to_owned(), notebook.to_owned());
    crate::sidecar::with_connection(&data_dir, move |conn| {
        conn.execute(
            "UPDATE projects SET notebook = ?1 WHERE id = ?2",
            [notebook, id],
        )
        .map(drop)
    })
    .await
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn wall_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

fn store_target(target: &Target) -> String {
    match target {
        Target::Static(folder) => path_text(folder),
        Target::Origin(origin) => origin.clone(),
    }
}

fn read_target(text: String) -> Target {
    if text.starts_with("http://") || text.starts_with("https://") {
        Target::Origin(text)
    } else {
        Target::Static(PathBuf::from(text))
    }
}

fn project_of_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    let port: i64 = row.get(4)?;
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        root: PathBuf::from(row.get::<_, String>(2)?),
        target: read_target(row.get(3)?),
        port: u16::try_from(port).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(4, port))?,
        notebook: row.get(5)?,
    })
}
