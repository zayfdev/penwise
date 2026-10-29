//! `.penwise.json`: the link record next to a project's source.
//!
//! It is written by `penwise link` and read by every tool that needs to know which project a folder belongs to.
//! `project` is the registry id, which is what `DELETE /projects/:id` takes.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// The file name, in the project root.
pub const FILE: &str = ".penwise.json";

/// The contents of `.penwise.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    /// The registry id.
    pub project: String,
    /// The project's display name.
    pub name: String,
    /// The folder Penwise serves, for a static project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#static: Option<String>,
    /// The dev server origin, for a proxied project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

impl Link {
    /// Writes the file into `dir` and returns its path.
    ///
    /// # Errors
    /// The directory is not writable.
    pub fn save(&self, dir: &std::path::Path) -> Result<PathBuf, Error> {
        let path = dir.join(FILE);
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        fs::write(&path, text)?;
        Ok(path)
    }

    /// Reads the file from `dir`.
    ///
    /// # Errors
    /// No `.penwise.json` there (the folder is not linked), or it is unreadable.
    pub fn load(dir: &std::path::Path) -> Result<Self, Error> {
        let path = dir.join(FILE);
        let text = fs::read_to_string(&path).map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                Error::NotLinked(dir.to_path_buf())
            } else {
                Error::Io(error)
            }
        })?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Removes the file.
    ///
    /// # Errors
    /// The file could not be removed.
    pub fn remove(dir: &std::path::Path) -> Result<(), Error> {
        fs::remove_file(dir.join(FILE))?;
        Ok(())
    }

    /// Whether `dir` already carries a link record.
    #[must_use]
    pub fn exists(dir: &std::path::Path) -> bool {
        dir.join(FILE).is_file()
    }
}
