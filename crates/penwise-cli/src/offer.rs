//! The agent files `penwise link` offers beyond the MCP config (spec v2 §6): the `penwise inbox` hook for Claude
//! Code and the `penwise` skill for Claude Code and omp. Each is asked first with the exact change shown, merged
//! into what is already there, and recorded in `.penwise.json` so `unlink` can take out exactly that.
//!
//! A failure here is reported and does not undo the link: the project is registered either way.

use std::path::Path;

use crate::diff;
use crate::hook;
use crate::link::ask;
use crate::project_file::Installed;
use crate::skill::{self, Agent};

/// Offers the `UserPromptSubmit` hook in `.claude/settings.json`.
pub fn hook(root: &Path, yes: bool, installed: &mut Installed) {
    let path = hook::settings_path(root);
    println!(
        "\nClaude Code hook: runs `penwise inbox` when you submit a prompt, so a round you sent is in context ({})",
        path.display()
    );
    let before = match hook::read(&path) {
        Ok(before) => before,
        Err(error) => return println!("  could not read the settings: {error}"),
    };
    let command = hook::command();
    let after = match hook::merged(before.as_deref(), &path, &command) {
        Ok(Some(after)) => after,
        Ok(None) => return println!("  already in {}", path.display()),
        Err(error) => return println!("  not changed: {error}"),
    };
    println!(
        "{}",
        diff::unified_labelled(
            &path,
            before.as_deref().unwrap_or(""),
            &after,
            "penwise hook"
        )
    );
    if !yes && !ask("add the hook to .claude/settings.json? [Y/n] ", true).unwrap_or(false) {
        return println!("  hook not added");
    }
    match write_file(&path, &after) {
        Ok(()) => {
            installed.hook = true;
            installed.hook_file_created |= before.is_none();
            println!("  wrote {}", path.display());
        }
        Err(error) => println!("  could not write the hook: {error}"),
    }
}

/// Offers the skill for both agents.
pub fn skills(root: &Path, yes: bool, installed: &mut Installed) {
    println!("\nPenwise skill for agents (how to work a design round):");
    let mut pending = Vec::new();
    for agent in Agent::ALL {
        let path = agent.path(root);
        match hook::read(&path) {
            Ok(Some(text)) if text == skill::TEXT => {
                println!("  {:<12} {} (up to date)", agent.label(), path.display());
            }
            Ok(Some(text)) => {
                println!("  {:<12} {} (differs)", agent.label(), path.display());
                println!(
                    "{}",
                    diff::unified_labelled(&path, &text, skill::TEXT, "penwise skill")
                );
                pending.push(agent);
            }
            Ok(None) => {
                println!(
                    "  {:<12} {} (new, {} lines)",
                    agent.label(),
                    path.display(),
                    skill::TEXT.lines().count()
                );
                pending.push(agent);
            }
            Err(error) => println!("  {:<12} unreadable: {error}", agent.label()),
        }
    }
    if pending.is_empty() {
        return;
    }
    if !yes && !ask("write the skill? [Y/n] ", true).unwrap_or(false) {
        return println!("  skill not written");
    }
    for agent in pending {
        match skill::write(root, agent) {
            Ok(path) => {
                match agent {
                    Agent::Claude => installed.claude_skill = true,
                    Agent::Omp => installed.omp_skill = true,
                }
                println!("  wrote {}: {}", agent.label(), path.display());
            }
            Err(error) => println!("  could not write the {} skill: {error}", agent.label()),
        }
    }
}

fn write_file(path: &Path, text: &str) -> Result<(), crate::error::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)?;
    Ok(())
}
