//! Linked projects: the registry rows and their live ports (design-harness spec §6.1, §8).
//!
//! A project is a folder the owner linked (`penwise link`); its row is server-local metadata beside the op log
//! ([`store`]), so it never syncs. Each row owns one port on the Penwise host, where its dev origin is proxied or
//! its static folder is served — see [`super`] for what a project port answers.
//!
//! Ports are stable: a new project takes the first free port from [`DEFAULT_PORT_BASE`] (7421), and the choice is
//! persisted. At boot every row is started again; a stored port some other program has taken is reallocated once
//! and the new port persisted. Adding, relinking and removing a project starts or stops its server immediately.

mod store;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use axum::http::StatusCode;
use penwise_proto::notebook::{MAX_TITLE_BYTES, POSITION_STEP};
use penwise_proto::{NotebookChange, NotebookId};
use serde::Deserialize;

use super::{FrameError, ProjectConfig, ProjectServer, Target};
use crate::api::{ApiError, Author};
use crate::storage::Storage;
use crate::storage::random::random_u128;

/// First project port; the main server keeps 7420 (`PENWISE_PROJECT_PORT` overrides the base).
pub const DEFAULT_PORT_BASE: u16 = 7421;
/// Longest project id, also the frame item's `project` limit (contract C1).
const MAX_ID: usize = 64;

/// A registered project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Project {
    pub(crate) id: String,
    pub(crate) name: String,
    /// The linked folder (canonical).
    pub(crate) root: PathBuf,
    /// What its port serves.
    pub(crate) target: Target,
    /// The port it listens on; persisted, so a restart reuses it.
    pub(crate) port: u16,
    /// The notebook created for it (`None` until the API creates one).
    pub(crate) notebook: Option<String>,
}

impl Project {
    /// `"static"` or `"proxy"`, the `kind` of `GET /projects`.
    pub(crate) fn kind(&self) -> &'static str {
        match self.target {
            Target::Static(_) => "static",
            Target::Origin(_) => "proxy",
        }
    }

    /// The folder `kind: "static"` serves.
    pub(crate) fn static_dir(&self) -> Option<&Path> {
        match &self.target {
            Target::Static(folder) => Some(folder),
            Target::Origin(_) => None,
        }
    }

    /// The dev origin `kind: "proxy"` forwards to.
    pub(crate) fn origin(&self) -> Option<&str> {
        match &self.target {
            Target::Static(_) => None,
            Target::Origin(origin) => Some(origin),
        }
    }
}

/// A `POST /projects` body, validated by [`Registry::link`].
#[derive(Debug, Default, Deserialize, Clone)]
pub(crate) struct LinkRequest {
    pub(crate) id: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) root: PathBuf,
    /// `static` in JSON: the folder served by Penwise itself.
    #[serde(rename = "static")]
    pub(crate) folder: Option<PathBuf>,
    /// `origin` in JSON: a dev server origin.
    pub(crate) origin: Option<String>,
}

/// The rows plus their live servers. Cheap to clone through an [`Arc`].
pub(crate) struct Registry {
    storage: Storage,
    data_dir: Arc<PathBuf>,
    token: Arc<String>,
    host: String,
    base_port: u16,
    servers: Mutex<std::collections::BTreeMap<String, ProjectServer>>,
    /// Serialises link/remove/boot so one project is never started twice.
    changes: tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Registry")
            .field("host", &self.host)
            .field("base_port", &self.base_port)
            .finish_non_exhaustive()
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        for (_, server) in std::mem::take(&mut *self.lock_servers()) {
            server.abort();
        }
    }
}

impl Registry {
    pub(crate) fn new(
        storage: Storage,
        data_dir: Arc<PathBuf>,
        token: Arc<String>,
        host: String,
        base_port: u16,
    ) -> Self {
        Self {
            storage,
            data_dir,
            token,
            host,
            base_port,
            servers: Mutex::new(std::collections::BTreeMap::new()),
            changes: tokio::sync::Mutex::new(()),
        }
    }

    /// Starts every persisted project, reusing its port when free. A project that cannot start is logged and
    /// skipped, so one broken folder never keeps the server down.
    pub(crate) async fn start_all(&self) {
        let _changing = self.changes.lock().await;
        let projects = match self.list().await {
            Ok(projects) => projects,
            Err(error) => {
                tracing::error!(reason = %error.message, "project registry unreadable");
                return;
            }
        };
        for project in projects {
            let mut started = project.clone();
            if let Err(error) = self.launch(&mut started).await {
                tracing::warn!(
                    project = %project.id,
                    reason = %error.message,
                    "project port not started"
                );
                continue;
            }
            if started.port != project.port {
                let mut updated = project;
                updated.port = started.port;
                if let Err(error) = store::update(&self.data_dir, &updated).await {
                    tracing::warn!(project = %updated.id, reason = %error.message, "project port not saved");
                }
            }
            tracing::info!(project = %started.id, port = started.port, "project port listening");
        }
    }

    /// Every registered project, in link order.
    pub(crate) async fn list(&self) -> Result<Vec<Project>, ApiError> {
        store::list(&self.data_dir).await
    }

    /// One registered project.
    pub(crate) async fn get(&self, id: &str) -> Result<Option<Project>, ApiError> {
        store::get(&self.data_dir, id).await
    }

    /// Registers a project and starts its port; idempotent by `root` (a re-link returns the same row, updated).
    ///
    /// # Errors
    /// 400 for a malformed id, name, folder or origin; 409 when `id` and `root` name different projects; 500 when
    /// the port cannot be bound or the row cannot be saved.
    pub(crate) async fn link(&self, request: LinkRequest) -> Result<Project, ApiError> {
        let _changing = self.changes.lock().await;
        let (root, target, name, id) = resolve(request).await?;
        let known = self.list().await?;
        let by_root = known.iter().find(|project| project.root == root);
        let by_id = id
            .as_ref()
            .and_then(|id| known.iter().find(|project| &project.id == id));
        if let (Some(same_root), Some(same_id)) = (by_root, by_id)
            && same_root.id != same_id.id
        {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "that id and root name different projects",
            ));
        }
        let existing = by_root.or(by_id).cloned();
        let registered = existing.is_some();
        let mut project = match existing {
            Some(existing) => Project {
                name,
                root,
                target,
                ..existing
            },
            None => Project {
                id: match id {
                    Some(id) => checked_id(&id)?,
                    None => format!("prj_{:032x}", random_u128()),
                },
                name,
                root,
                target,
                port: self.allocate(&ports_of(&known)).await?,
                notebook: None,
            },
        };
        let target_changed = known
            .iter()
            .any(|known| known.id == project.id && known.target != project.target);
        if target_changed || !self.running(&project.id) {
            self.stop_server(&project.id).await;
            self.launch(&mut project).await?;
        }
        if registered {
            store::update(&self.data_dir, &project).await?;
        } else {
            store::insert(&self.data_dir, &project).await?;
        }
        if project.notebook.is_none() {
            let notebook = self.create_notebook(&project.name).await?;
            store::set_notebook(&self.data_dir, &project.id, &notebook).await?;
            project.notebook = Some(notebook);
        }
        Ok(project)
    }

    /// Removes a project and stops its port. `false` when it was not registered.
    pub(crate) async fn remove(&self, id: &str) -> Result<bool, ApiError> {
        let _changing = self.changes.lock().await;
        if self.get(id).await?.is_none() {
            return Ok(false);
        }
        self.stop_server(id).await;
        store::delete(&self.data_dir, id).await?;
        Ok(true)
    }

    /// The notebook a linked project gets: named after it, appended to the library. Created under the changes
    /// lock, so two concurrent links cannot make two notebooks for one project.
    async fn create_notebook(&self, name: &str) -> Result<String, ApiError> {
        let catalog = self.storage.catalog().await?;
        let position = catalog
            .notebooks()
            .last()
            .map_or(POSITION_STEP, |notebook| notebook.position + POSITION_STEP);
        let id = NotebookId::from_parts(self.storage.now_ms(), random_u128());
        let mut author = Author::new(&self.storage);
        author
            .notebook(
                id,
                NotebookChange::Create {
                    title: notebook_title(name),
                    position,
                },
            )
            .await?;
        author.commit().await?;
        Ok(id.to_string())
    }

    /// Stops every project port (server shutdown).
    pub(crate) async fn shutdown(&self) {
        let servers: Vec<ProjectServer> = {
            let mut map = self.lock_servers();
            std::mem::take(&mut *map).into_values().collect()
        };
        for server in servers {
            server.shutdown().await;
        }
    }

    fn lock_servers(&self) -> MutexGuard<'_, std::collections::BTreeMap<String, ProjectServer>> {
        self.servers.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn running(&self, id: &str) -> bool {
        self.lock_servers().contains_key(id)
    }

    async fn stop_server(&self, id: &str) {
        let server = self.lock_servers().remove(id);
        if let Some(server) = server {
            server.shutdown().await;
        }
    }

    /// Starts `project` on its stored port, else on the first free port; records the live port in `project`.
    async fn launch(&self, project: &mut Project) -> Result<(), ApiError> {
        let mut skip = self.ports_in_use().await?;
        let mut next = Some(project.port);
        loop {
            let port = match next {
                Some(port) => port,
                None => self.allocate(&skip).await?,
            };
            skip.insert(port);
            match self.start_at(project, port).await {
                Ok(server) => {
                    project.port = server.addr.port();
                    self.lock_servers().insert(project.id.clone(), server);
                    return Ok(());
                }
                Err(FrameError::Io(error)) if error.kind() == std::io::ErrorKind::AddrInUse => {
                    next = None;
                }
                Err(FrameError::Config(message)) => return Err(ApiError::bad_request(message)),
                Err(error) => {
                    return Err(ApiError::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("project port: {error}"),
                    ));
                }
            }
        }
    }

    async fn start_at(&self, project: &Project, port: u16) -> Result<ProjectServer, FrameError> {
        let config = match &project.target {
            Target::Static(folder) => ProjectConfig::static_site(
                project.id.clone(),
                self.host.clone(),
                port,
                folder.clone(),
            ),
            Target::Origin(origin) => ProjectConfig::dev_origin(
                project.id.clone(),
                self.host.clone(),
                port,
                origin.clone(),
            ),
        };
        ProjectServer::start(config, (*self.token).clone()).await
    }

    /// The first port at or above the base that no project claims and no socket holds.
    async fn allocate(&self, skip: &BTreeSet<u16>) -> Result<u16, ApiError> {
        let mut port = self.base_port.max(1);
        loop {
            if !skip.contains(&port) && port_free(&self.host, port).await {
                return Ok(port);
            }
            port = port.checked_add(1).ok_or_else(|| {
                ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "no free project port")
            })?;
        }
    }

    async fn ports_in_use(&self) -> Result<BTreeSet<u16>, ApiError> {
        Ok(ports_of(&self.list().await?))
    }
}

fn ports_of(projects: &[Project]) -> BTreeSet<u16> {
    projects.iter().map(|project| project.port).collect()
}

/// The project name, trimmed to what a notebook title accepts (a name is not otherwise restricted).
fn notebook_title(name: &str) -> String {
    let trimmed = name.trim();
    let mut title = if trimmed.is_empty() {
        "Project".to_owned()
    } else {
        trimmed.to_owned()
    };
    if title.len() > MAX_TITLE_BYTES {
        let mut end = MAX_TITLE_BYTES;
        while !title.is_char_boundary(end) {
            end -= 1;
        }
        title.truncate(end);
    }
    title
}

/// Canonicalises the request and checks the parts a bad link would otherwise store broken.
async fn resolve(
    request: LinkRequest,
) -> Result<(PathBuf, Target, String, Option<String>), ApiError> {
    let target = match (request.folder, request.origin.as_deref()) {
        (Some(folder), None) => {
            let folder = tokio::fs::canonicalize(&folder).await.map_err(|error| {
                ApiError::bad_request(format!("static folder {}: {error}", folder.display()))
            })?;
            directory(&folder, "static folder")?;
            Target::Static(folder)
        }
        (None, Some(origin)) => {
            super::proxy::Proxy::new(origin)
                .map_err(|error| ApiError::bad_request(error.to_string()))?;
            Target::Origin(origin.trim().trim_end_matches('/').to_owned())
        }
        _ => {
            return Err(ApiError::bad_request(
                "exactly one of `static` (a folder) or `origin` (a dev server) is required",
            ));
        }
    };
    let root = tokio::fs::canonicalize(&request.root)
        .await
        .map_err(|error| {
            ApiError::bad_request(format!("root {}: {error}", request.root.display()))
        })?;
    directory(&root, "root")?;
    let name = match request.name {
        Some(name) if !name.trim().is_empty() => name.trim().to_owned(),
        Some(_) => return Err(ApiError::bad_request("name is empty")),
        None => root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| ApiError::bad_request("root has no folder name; send `name`"))?,
    };
    let id = request.id.map(|id| checked_id(&id)).transpose()?;
    Ok((root, target, name, id))
}

fn directory(path: &Path, what: &str) -> Result<(), ApiError> {
    let is_dir = std::fs::metadata(path).is_ok_and(|meta| meta.is_dir());
    if is_dir {
        Ok(())
    } else {
        Err(ApiError::bad_request(format!(
            "{what} {} is not a directory",
            path.display()
        )))
    }
}

fn checked_id(id: &str) -> Result<String, ApiError> {
    let valid = !id.is_empty()
        && id.len() <= MAX_ID
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if valid {
        Ok(id.to_owned())
    } else {
        Err(ApiError::bad_request(
            "id is 1–64 characters of ASCII letters, digits, `-` or `_`",
        ))
    }
}

async fn port_free(host: &str, port: u16) -> bool {
    tokio::net::TcpListener::bind((host, port)).await.is_ok()
}
