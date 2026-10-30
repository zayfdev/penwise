//! The Claude Code hook `penwise link` offers: a `UserPromptSubmit` entry in `.claude/settings.json` that runs
//! `penwise inbox` (spec v2 §6). Claude Code adds a `UserPromptSubmit` hook's stdout to the prompt's context, so
//! when the owner submits any prompt after tapping Send, the round is already in context.
//!
//! The settings file may hold the owner's own keys and hooks, so everything here is a merge: other keys and
//! other hooks survive untouched, a second `link` adds nothing, and `unlink` removes only the entry whose
//! command is `penwise inbox`.

use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::error::Error;

/// The hook event whose stdout Claude Code adds to the prompt.
pub const EVENT: &str = "UserPromptSubmit";
/// Where the settings live, relative to the project root.
pub const SETTINGS: &str = ".claude/settings.json";

/// The settings file of the project in `root`.
#[must_use]
pub fn settings_path(root: &Path) -> PathBuf {
    root.join(SETTINGS)
}

/// The command the hook runs.
///
/// Plain `penwise inbox` when a `penwise` executable is on `PATH`: the settings file is usually committed with the
/// project and shared, and a portable command works on every machine that installed Penwise (hooks run in the
/// user's own `PATH`). Otherwise the absolute path of the running binary, quoted, because a bare name would
/// fail silently there.
#[must_use]
pub fn command() -> String {
    command_for(
        std::env::var_os("PATH").as_deref(),
        std::env::current_exe().ok().as_deref(),
    )
}

/// [`command`] with its two inputs given (tests).
#[must_use]
pub fn command_for(path_var: Option<&OsStr>, exe: Option<&Path>) -> String {
    let on_path = path_var.is_some_and(|paths| {
        std::env::split_paths(paths).any(|dir| {
            let candidate = dir.join("penwise");
            candidate.is_file()
        })
    });
    match (on_path, exe) {
        (false, Some(exe)) => format!("\"{}\" inbox", exe.display()),
        _ => "penwise inbox".to_owned(),
    }
}

/// Whether a hook command is the one this module writes: `penwise inbox`, or a path to a binary named `penwise`
/// followed by `inbox`, with optional quotes.
#[must_use]
pub fn is_ours(command: &str) -> bool {
    let trimmed = command.trim();
    let Some(program) = trimmed.strip_suffix(" inbox") else {
        return false;
    };
    let program = program.trim().trim_matches('"').trim_matches('\'');
    Path::new(program).file_name() == Some(OsStr::new("penwise"))
}

fn parse(text: Option<&str>) -> Result<Value, Error> {
    match text {
        Some(text) if !text.trim().is_empty() => Ok(serde_json::from_str(text)?),
        _ => Ok(Value::Object(Map::new())),
    }
}

fn shape(path: &Path, what: &str) -> Error {
    Error::Config(format!("{}: {what}", path.display()))
}

/// Reads `path`; `None` when it does not exist.
///
/// # Errors
/// The file exists but cannot be read.
pub fn read(path: &Path) -> Result<Option<String>, Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::Io(error)),
    }
}

/// `before` with the hook added, or `None` when a `penwise inbox` hook is already there.
///
/// # Errors
/// `before` is not a JSON object, or `hooks` / `hooks.UserPromptSubmit` has an unexpected shape. Nothing is
/// overwritten then: settings the owner wrote by hand are worth more than a generated hook.
pub fn merged(before: Option<&str>, path: &Path, command: &str) -> Result<Option<String>, Error> {
    let mut root = parse(before)?;
    let object = root
        .as_object_mut()
        .ok_or_else(|| shape(path, "not a JSON object"))?;
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| shape(path, "\"hooks\" is not an object"))?;
    let groups = hooks
        .entry(EVENT)
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| shape(path, "\"hooks.UserPromptSubmit\" is not an array"))?;
    if groups.iter().any(has_ours) {
        return Ok(None);
    }
    groups.push(json!({ "hooks": [{ "type": "command", "command": command }] }));
    let mut text = serde_json::to_string_pretty(&root)?;
    text.push('\n');
    Ok(Some(text))
}

fn has_ours(group: &Value) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| {
            hooks.iter().any(|hook| {
                hook.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(is_ours)
            })
        })
}

/// What removing the hook leaves.
#[derive(Debug, PartialEq, Eq)]
pub enum Removal {
    /// No `penwise inbox` hook there.
    Nothing,
    /// The file without the hook (and without the containers it emptied).
    Rewritten(String),
    /// The file would be an empty object.
    Empty,
}

/// `text` without the `penwise inbox` hook; containers that become empty are dropped, everything else stays.
///
/// # Errors
/// `text` is not JSON, or the hooks have an unexpected shape.
pub fn removed(text: &str, path: &Path) -> Result<Removal, Error> {
    let mut root = parse(Some(text))?;
    let Some(object) = root.as_object_mut() else {
        return Err(shape(path, "not a JSON object"));
    };
    let Some(Value::Object(hooks)) = object.get_mut("hooks") else {
        return Ok(Removal::Nothing);
    };
    let Some(Value::Array(groups)) = hooks.get_mut(EVENT) else {
        return Ok(Removal::Nothing);
    };
    let mut found = false;
    for group in groups.iter_mut() {
        if let Some(Value::Array(inner)) = group.get_mut("hooks") {
            let count = inner.len();
            inner.retain(|hook| {
                !hook
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(is_ours)
            });
            found |= inner.len() != count;
        }
    }
    groups.retain(|group| {
        !group
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    });
    if !found {
        return Ok(Removal::Nothing);
    }
    if groups.is_empty() {
        hooks.remove(EVENT);
    }
    if hooks.is_empty() {
        object.remove("hooks");
    }
    if object.is_empty() {
        return Ok(Removal::Empty);
    }
    let mut out = serde_json::to_string_pretty(&root)?;
    out.push('\n');
    Ok(Removal::Rewritten(out))
}

#[cfg(test)]
mod tests;
