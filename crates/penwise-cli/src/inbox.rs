//! `penwise inbox`: one line when a sent round is waiting for the agent, nothing otherwise.
//!
//! It runs inside an agent's prompt hook (spec v2 §6), so it must never get in the way: it prints the server's
//! line or nothing, exits 0 whatever happens, and gives up after [`TIMEOUT`] so a stopped server costs the
//! prompt next to nothing. The project is `PENWISE_PROJECT`, else the `.penwise.json` of the current folder or
//! the nearest ancestor that has one. The route is in the `/projects` group, which accepts a same-machine
//! request without the token, so there is no pairing round trip.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::client;
use crate::project_file;

/// The whole request, connect included. A hook that waits longer than this is worse than no hook.
pub const TIMEOUT: Duration = Duration::from_millis(300);

/// The server's answer.
#[derive(Debug, Deserialize)]
struct Inbox {
    #[serde(default)]
    line: Option<String>,
}

/// Prints the inbox line of the current project, when there is one. Never fails.
pub fn run() {
    let Some(project) = project_id() else {
        return;
    };
    if let Some(line) = fetch(&client::base_url(), &project) {
        println!("{line}");
    }
}

/// `PENWISE_PROJECT`, else the link record found from the current folder upwards.
fn project_id() -> Option<String> {
    if let Ok(project) = std::env::var("PENWISE_PROJECT")
        && !project.trim().is_empty()
    {
        return Some(project.trim().to_owned());
    }
    let start = std::env::current_dir().ok()?;
    find_link(&start)
}

/// The project id of the nearest `.penwise.json` at or above `start`.
#[must_use]
pub fn find_link(start: &Path) -> Option<String> {
    let mut dir: Option<PathBuf> = Some(start.to_path_buf());
    while let Some(current) = dir {
        if project_file::Link::exists(&current) {
            return project_file::Link::load(&current)
                .ok()
                .map(|link| link.project)
                .filter(|project| !project.is_empty());
        }
        dir = current.parent().map(Path::to_path_buf);
    }
    None
}

/// `GET <base>/projects/<project>/inbox`'s line; `None` for no line, a down server, a slow one or any error.
#[must_use]
pub fn fetch(base: &str, project: &str) -> Option<String> {
    let http = reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .connect_timeout(TIMEOUT)
        .no_proxy()
        .build()
        .ok()?;
    let url = format!(
        "{}/projects/{}/inbox",
        base.trim_end_matches('/'),
        segment(project)
    );
    let response = http.get(url).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let inbox: Inbox = response.json().ok()?;
    inbox.line.filter(|line| !line.trim().is_empty())
}

/// A project id as one URL path segment.
fn segment(id: &str) -> String {
    id.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests;
