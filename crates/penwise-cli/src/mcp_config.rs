//! The agent config `penwise link` writes (design-harness spec §8 step 5).
//!
//! Two clients discover project MCP servers from files in the project root: Claude Code reads `.mcp.json`, and
//! omp reads `.omp/mcp.json` (its native project file; it also falls back to a root `.mcp.json`). Both use the
//! same `mcpServers` map, so both files get the same `penwise` entry and every other member is kept.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::error::Error;

/// The server name the agent sees (and the key both files use).
pub const SERVER: &str = "penwise";
/// The MCP binary `scripts/install-desktop.sh` installs, relative to `$HOME`.
const BINARY: &str = ".local/bin/penwise-mcp";
/// The server's API port.
const PORT: &str = "7420";
/// omp's schema URL, which its own writer adds to every file it owns.
const SCHEMA: &str = "https://raw.githubusercontent.com/can1357/oh-my-pi/main/packages/coding-agent/src/config/mcp-schema.json";

/// The files `link` maintains for one project root.
#[derive(Debug, Clone)]
pub struct Targets {
    /// Claude Code's project config.
    pub claude: PathBuf,
    /// omp's native project config.
    pub omp: PathBuf,
}

impl Targets {
    /// The two paths under `root`.
    #[must_use]
    pub fn of(root: &Path) -> Self {
        Self {
            claude: root.join(".mcp.json"),
            omp: root.join(".omp").join("mcp.json"),
        }
    }

    /// Both files, in the order `link` prints them.
    #[must_use]
    pub fn all(&self) -> [(&'static str, &Path); 2] {
        [("Claude Code", &self.claude), ("omp", &self.omp)]
    }
}

/// The `penwise` server entry, as both clients read it.
#[must_use]
pub fn server_entry(project: &str) -> Value {
    json!({
        "command": mcp_binary(),
        "env": { "PENWISE_PORT": PORT, "PENWISE_PROJECT": project },
    })
}

/// `$HOME/.local/bin/penwise-mcp` (the installer's fixed path), else the unexpanded `~` form.
fn mcp_binary() -> String {
    std::env::var_os("HOME").map_or_else(
        || format!("~/{BINARY}"),
        |home| {
            PathBuf::from(home)
                .join(BINARY)
                .to_string_lossy()
                .into_owned()
        },
    )
}

/// `path`'s contents with the `penwise` entry merged in, plus omp's schema when `schema` is set.
///
/// # Errors
/// The file is unreadable or not a JSON object, or `mcpServers` is not one. Nothing is overwritten in that
/// case: a config the owner wrote by hand is worth more than a generated one.
pub fn merged(path: &Path, project: &str, schema: bool) -> Result<String, Error> {
    let mut root = match std::fs::read_to_string(path) {
        Ok(text) if !text.trim().is_empty() => serde_json::from_str::<Value>(&text)?,
        Ok(_) => Value::Object(Map::new()),
        Err(error) if error.kind() == ErrorKind::NotFound => Value::Object(Map::new()),
        Err(error) => return Err(Error::Io(error)),
    };
    let object = root
        .as_object_mut()
        .ok_or_else(|| Error::Config(format!("{} is not a JSON object", path.display())))?;
    let servers = object
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let servers = servers.as_object_mut().ok_or_else(|| {
        Error::Config(format!(
            "{}: \"mcpServers\" is not an object",
            path.display()
        ))
    })?;
    servers.insert(SERVER.to_owned(), server_entry(project));
    if schema {
        object
            .entry("$schema")
            .or_insert_with(|| Value::String(SCHEMA.to_owned()));
    }
    let mut text = serde_json::to_string_pretty(&root)?;
    text.push('\n');
    Ok(text)
}

/// Writes both files for `project`, creating `.omp/` when needed. Returns what was written, in print order.
///
/// # Errors
/// Either file cannot be merged or written; `link` reports that and keeps the link itself.
pub fn write(root: &Path, project: &str) -> Result<Vec<(&'static str, PathBuf)>, Error> {
    let targets = Targets::of(root);
    let mut written = Vec::with_capacity(2);
    for (client, path, schema) in [
        ("Claude Code", &targets.claude, false),
        ("omp", &targets.omp, true),
    ] {
        let text = merged(path, project, schema)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)?;
        written.push((client, path.clone()));
    }
    Ok(written)
}

#[cfg(test)]
mod tests;
