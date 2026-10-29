//! What a folder is: a static site Penwise serves, or a dev server it proxies (design-harness spec §8, step 1).
//!
//! The signals are the ones a frontend developer would look for: a framework config file, a `package.json`
//! `dev` script, then an `index.html` at the root or in the conventional output folder. A dev server wins over
//! a built `dist/`: a Vite project has a root `index.html` *and* a dev server, and HMR is the point.

mod dev;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use crate::error::Error;
use dev::{framework_in_script, framework_of, port_in_config, port_in_script, read_package};

/// The dev servers bind loopback; the Penwise proxy dials the same address.
const HOST: &str = "127.0.0.1";
/// Port for a `dev` script whose framework Penwise cannot name.
const GENERIC_DEV_PORT: u16 = 5173;
/// Static folders Penwise serves when the root has no `index.html`, in preference order.
const STATIC_DIRS: &[&str] = &["site", "public", "dist"];

/// How Penwise reaches the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A folder served by Penwise itself, with live reload.
    Static { folder: PathBuf },
    /// A dev server origin (`http://127.0.0.1:5173`), reverse-proxied.
    Dev { origin: String },
}

/// A detected project: what `penwise link` registers and writes to `.penwise.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    /// The canonical folder the link belongs to.
    pub root: PathBuf,
    pub name: String,
    pub kind: Kind,
    /// The framework whose default port applied, when one was recognised.
    pub framework: Option<&'static str>,
}

impl Detected {
    /// The folder Penwise serves (static projects only).
    #[must_use]
    pub fn static_folder(&self) -> Option<&Path> {
        match &self.kind {
            Kind::Static { folder } => Some(folder),
            Kind::Dev { .. } => None,
        }
    }

    /// The dev server origin (dev projects only).
    #[must_use]
    pub fn origin(&self) -> Option<&str> {
        match &self.kind {
            Kind::Dev { origin } => Some(origin),
            Kind::Static { .. } => None,
        }
    }

    /// One line for the confirmation prompt.
    #[must_use]
    pub fn describe(&self) -> String {
        match &self.kind {
            Kind::Static { folder } => format!("static site in {}", folder.display()),
            Kind::Dev { origin } => self.framework.map_or_else(
                || format!("dev server at {origin}"),
                |name| format!("{name} dev server at {origin}"),
            ),
        }
    }
}

/// Detects what `dir` is.
///
/// # Errors
/// The folder does not exist, or has neither a dev server nor an `index.html`.
pub fn detect(dir: &Path) -> Result<Detected, Error> {
    let root = dir
        .canonicalize()
        .map_err(|error| Error::Undetected(format!("{}: {error}", dir.display())))?;
    if !root.is_dir() {
        return Err(Error::Undetected(format!(
            "{} is not a folder",
            root.display()
        )));
    }
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("project")
        .to_owned();
    let package = read_package(&root);
    let script = package.as_ref().and_then(dev::Package::dev_script);
    let framework =
        framework_of(&root, package.as_ref()).or_else(|| script.and_then(framework_in_script));
    if let Some(framework) = framework {
        let port = script
            .and_then(port_in_script)
            .or_else(|| port_in_config(&root, framework))
            .unwrap_or(framework.default_port);
        return Ok(Detected {
            root,
            name,
            kind: dev(port),
            framework: Some(framework.name),
        });
    }
    if let Some(script) = script {
        return Ok(Detected {
            root,
            name,
            kind: dev(port_in_script(script).unwrap_or(GENERIC_DEV_PORT)),
            framework: None,
        });
    }
    if let Some(folder) = static_folder(&root) {
        return Ok(Detected {
            root,
            name,
            kind: Kind::Static { folder },
            framework: None,
        });
    }
    Err(Error::Undetected(format!(
        "{} has no index.html at the root or in {}, and no dev script in package.json",
        root.display(),
        STATIC_DIRS.join(", ")
    )))
}

fn dev(port: u16) -> Kind {
    Kind::Dev {
        origin: format!("http://{HOST}:{port}"),
    }
}

/// `index.html` at the root, else the first conventional output folder that has one.
fn static_folder(root: &Path) -> Option<PathBuf> {
    if root.join("index.html").is_file() {
        return Some(root.to_path_buf());
    }
    STATIC_DIRS
        .iter()
        .map(|dir| root.join(dir))
        .find(|folder| folder.join("index.html").is_file())
}
