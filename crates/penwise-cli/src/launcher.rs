//! `penwise` / `penwise open`: the desktop window (ported from the removed `desktop/penwise` script).
//!
//! Makes sure the user service runs — the server is started from login on, but a crash loop or a fresh install
//! can leave it down — then replaces this process with the browser as its own app window.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

use crate::client;
use crate::error::Error;

const SERVICE: &str = "penwise.service";
/// 100 × 100 ms: about what the script allowed the server to come up.
const WAIT_STEPS: u32 = 100;
const STEP: Duration = Duration::from_millis(100);
/// Tried in this order; `PENWISE_BROWSER` overrides all of them.
const BROWSERS: &[&str] = &["chromium", "google-chrome-stable", "chromium-browser"];

/// Opens the Penwise window, passing `extra` to the browser (its own arguments).
///
/// # Errors
/// The server did not come up, `HOME` is unset, or no browser could be started.
pub fn open(extra: &[String]) -> Result<(), Error> {
    let port = client::port();
    let url = format!("http://127.0.0.1:{port}/");
    if !reachable(&url) {
        ensure_service(port, &url)?;
    }
    browser(&url, extra)
}

fn reachable(url: &str) -> bool {
    client::answers(&format!("{url}pair/local"))
}

/// Starts the user service, then waits for it to answer.
fn ensure_service(port: u16, url: &str) -> Result<(), Error> {
    // A crash loop (port taken, bad data dir) leaves the unit failed and refusing starts until reset.
    let _ = systemctl(&["reset-failed", SERVICE]);
    let started = if std::env::var_os("PENWISE_PORT").is_some() {
        // The unit does not name a port; the user manager's environment reaches it at start.
        let _ = systemctl(&["set-environment", &format!("PENWISE_PORT={port}")]);
        systemctl(&["restart", SERVICE])
    } else {
        systemctl(&["start", SERVICE])
    };
    if !started {
        return Err(fail(
            "Penwise server did not start. Check: journalctl --user -u penwise -e",
        ));
    }
    for _ in 0..WAIT_STEPS {
        if reachable(url) {
            return Ok(());
        }
        sleep(STEP);
    }
    Err(fail(&format!(
        "Penwise server is not answering on port {port}. Check: journalctl --user -u penwise -e"
    )))
}

fn systemctl(args: &[&str]) -> bool {
    Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .is_ok_and(|status| status.success())
}

/// Replaces this process with the browser: the window outlives the launcher, like `exec` in the script.
fn browser(url: &str, extra: &[String]) -> Result<(), Error> {
    let Some(executable) = browser_path() else {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        let error = exec(command);
        return Err(Error::Launch(format!("cannot open {url}: {error}")));
    };
    let profile = profile_dir()?;
    let mut command = Command::new(&executable);
    command
        .arg(format!("--app={url}"))
        .arg("--class=Penwise")
        .arg(format!("--user-data-dir={}", profile.display()))
        .args(extra);
    let error = exec(command);
    Err(Error::Launch(format!(
        "cannot open {url} in {}: {error}",
        executable.display()
    )))
}

fn exec(mut command: Command) -> std::io::Error {
    std::os::unix::process::CommandExt::exec(&mut command)
}

/// `PENWISE_BROWSER`, else the first known Chromium on `PATH`.
fn browser_path() -> Option<PathBuf> {
    match std::env::var_os("PENWISE_BROWSER") {
        Some(name) if !name.is_empty() => Some(PathBuf::from(name)),
        _ => BROWSERS.iter().find_map(|name| on_path(name)),
    }
}

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Its own profile: a separate window class (launcher icon, alt-tab entry) and storage, apart from browsing.
fn profile_dir() -> Result<PathBuf, Error> {
    let home =
        std::env::var_os("HOME").ok_or_else(|| Error::Launch("HOME is not set".to_owned()))?;
    Ok(PathBuf::from(home).join(".local/share/penwise-app/browser"))
}

/// A terminal-less failure needs a notification: the launcher runs from a desktop entry.
fn fail(message: &str) -> Error {
    let _ = Command::new("notify-send")
        .args(["-a", "Penwise", "-u", "critical", "Penwise", message])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Error::Launch(message.to_owned())
}
