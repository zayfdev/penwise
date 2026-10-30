//! The skill `penwise link` offers: how an agent works a design round (spec v2 §6). It replaced the old
//! agent-prompt document: the same text is written where each agent reads skills.
//!
//! - Claude Code: `.claude/skills/penwise/SKILL.md`.
//! - omp: `.omp/skills/penwise/SKILL.md`. omp keeps a project's skills in `.omp/skills/<name>/SKILL.md` (the
//!   layout of the existing `.omp/skills` folders on this machine, with `skills.enabled: true` in its config), and
//!   on this machine its Claude-config provider is disabled, so it does not read `.claude/skills`. omp has hook
//!   and extension flags but no documented prompt hook, so there is no omp hook and none is written.

use std::path::{Path, PathBuf};

use crate::error::Error;

/// The skill text, the single source for both agents.
pub const TEXT: &str = include_str!("../assets/SKILL.md");

/// An agent that reads skills from the project folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Omp,
}

impl Agent {
    /// Both agents, in the order `link` prints them.
    pub const ALL: [Self; 2] = [Self::Claude, Self::Omp];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Omp => "omp",
        }
    }

    /// The agent's config folder in the project: `.claude` or `.omp`.
    fn folder(self) -> &'static str {
        match self {
            Self::Claude => ".claude",
            Self::Omp => ".omp",
        }
    }

    /// `<root>/.claude/skills/penwise/SKILL.md` or the omp twin.
    #[must_use]
    pub fn path(self, root: &Path) -> PathBuf {
        root.join(self.folder())
            .join("skills")
            .join("penwise")
            .join("SKILL.md")
    }
}

/// Writes the skill for `agent`, creating the folders.
///
/// # Errors
/// The folders or the file cannot be written.
pub fn write(root: &Path, agent: Agent) -> Result<PathBuf, Error> {
    let path = agent.path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, TEXT)?;
    Ok(path)
}

/// What removing the skill did.
#[derive(Debug, PartialEq, Eq)]
pub enum Removed {
    /// There was no such file.
    Absent,
    /// The file was not the text `link` wrote (the owner edited it), so it stays.
    Edited,
    Deleted,
}

/// Deletes the skill file when it is still the text `link` wrote, then its folders when they are empty.
///
/// # Errors
/// The file cannot be read or removed.
pub fn remove(root: &Path, agent: Agent) -> Result<Removed, Error> {
    let path = agent.path(root);
    let Some(current) = crate::hook::read(&path)? else {
        return Ok(Removed::Absent);
    };
    if current != TEXT {
        return Ok(Removed::Edited);
    }
    std::fs::remove_file(&path)?;
    let mut dir = path.parent();
    for _ in 0..3 {
        let Some(folder) = dir else { break };
        if std::fs::remove_dir(folder).is_err() {
            break;
        }
        dir = folder.parent();
    }
    Ok(Removed::Deleted)
}

#[cfg(test)]
mod tests;
