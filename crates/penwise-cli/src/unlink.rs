//! `penwise unlink [dir]`: remove the registry entry, what `link` wrote for the agents, and the link record.
//!
//! The agent files are removed from the record `link` kept in `.penwise.json` (`installed`): the `penwise`
//! MCP entry, the `penwise inbox` hook and the two skill files. Nothing else in those files is touched, and a
//! skill the owner edited is left alone.

use std::path::Path;

use crate::client::{self, Client};
use crate::error::Error;
use crate::hook::{self, Removal};
use crate::mcp_config::{self, Targets};
use crate::project_file::{Installed, Link};
use crate::skill::{self, Agent, Removed};

/// Unlinks the project in `dir`.
///
/// # Errors
/// The folder is not linked, the server cannot be reached, or the file cannot be removed.
pub fn run(dir: &Path) -> Result<(), Error> {
    let link = Link::load(dir)?;
    let base = client::base_url();
    let client = Client::connect(&base)?;
    let removed = client.delete(&link.project)?;
    remove_agent_files(dir, &link.installed);
    Link::remove(dir)?;
    if removed {
        println!(
            "unlinked \"{}\" (project {}) from {base}",
            link.name, link.project
        );
    } else {
        println!(
            "removed {}; project {} was not registered on {base} any more",
            dir.join(crate::project_file::FILE).display(),
            link.project
        );
    }
    Ok(())
}

/// Takes out what `link` added for the agents. Each failure is reported and the rest goes on.
pub fn remove_agent_files(root: &Path, installed: &Installed) {
    if installed.mcp {
        let targets = Targets::of(root);
        for (client, path) in targets.all() {
            match remove_mcp(path) {
                Ok(true) => println!(
                    "  removed the penwise MCP entry from {client}: {}",
                    path.display()
                ),
                Ok(false) => {}
                Err(error) => println!("  could not clean {}: {error}", path.display()),
            }
        }
    }
    if installed.hook {
        match remove_hook(root, installed.hook_file_created) {
            Ok(true) => println!(
                "  removed the inbox hook from {}",
                hook::settings_path(root).display()
            ),
            Ok(false) => {}
            Err(error) => println!("  could not clean the hook: {error}"),
        }
    }
    for (agent, wrote) in [
        (Agent::Claude, installed.claude_skill),
        (Agent::Omp, installed.omp_skill),
    ] {
        if !wrote {
            continue;
        }
        match skill::remove(root, agent) {
            Ok(Removed::Deleted) => {
                println!(
                    "  removed the {} skill: {}",
                    agent.label(),
                    agent.path(root).display()
                );
            }
            Ok(Removed::Edited) => println!(
                "  kept {} (it was edited since link wrote it)",
                agent.path(root).display()
            ),
            Ok(Removed::Absent) => {}
            Err(error) => println!("  could not remove the {} skill: {error}", agent.label()),
        }
    }
}

/// Removes the `penwise` entry from one MCP file; deletes the file when nothing else is left in it.
fn remove_mcp(path: &Path) -> Result<bool, Error> {
    let Some(text) = hook::read(path)? else {
        return Ok(false);
    };
    match mcp_config::removed(&text, path)? {
        mcp_config::Removal::Nothing => Ok(false),
        mcp_config::Removal::Rewritten(after) => {
            std::fs::write(path, after)?;
            Ok(true)
        }
        mcp_config::Removal::Empty => {
            std::fs::remove_file(path)?;
            if let Some(parent) = path
                .parent()
                .filter(|parent| parent.file_name() == Some(std::ffi::OsStr::new(".omp")))
            {
                // `.omp/` goes only when nothing else lives in it (`remove_dir` refuses a folder with files).
                let _ = std::fs::remove_dir(parent);
            }
            Ok(true)
        }
    }
}

/// Removes the hook; deletes `.claude/settings.json` when `link` created it and nothing else is left.
fn remove_hook(root: &Path, created: bool) -> Result<bool, Error> {
    let path = hook::settings_path(root);
    let Some(text) = hook::read(&path)? else {
        return Ok(false);
    };
    match hook::removed(&text, &path)? {
        Removal::Nothing => Ok(false),
        Removal::Rewritten(after) => {
            std::fs::write(&path, after)?;
            Ok(true)
        }
        Removal::Empty if created => {
            std::fs::remove_file(&path)?;
            if let Some(parent) = path.parent() {
                let _ = std::fs::remove_dir(parent);
            }
            Ok(true)
        }
        Removal::Empty => {
            std::fs::write(&path, "{}\n")?;
            Ok(true)
        }
    }
}
