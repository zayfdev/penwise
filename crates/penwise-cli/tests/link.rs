//! `penwise link` / `unlink` / `status` against the real binary and a stand-in for the project registry (C2).
//!
//! The fake implements exactly the routes the CLI uses — `GET /pair/local`, the token check, `GET/POST /projects`,
//! `DELETE /projects/:id` — with the answer shapes of the contract, so the CLI is exercised end to end without
//! the server crate (its registry is a different slice).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::items_after_statements
)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

const TOKEN: &str = "test-token";
/// The rule for this repo's tests: never 7420 or 7421–7430.
const FIRST_PORT: u16 = 17421;
const LAST_PORT: u16 = 17460;
/// Reported by the fake as a project's port; never bound, so it cannot collide with anything.
const PROJECT_PORT: u16 = 7501;

fn main_binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_penwise"))
}

// ---------------------------------------------------------------- the fake registry

struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: String,
}

#[derive(Default)]
struct State {
    projects: Vec<Value>,
    posts: Vec<Value>,
    deleted: Vec<String>,
}

struct Fake {
    addr: SocketAddr,
    state: Arc<Mutex<State>>,
}

impl Fake {
    fn start() -> Self {
        let (listener, port) = bind();
        let state = Arc::new(Mutex::new(State::default()));
        let serving = Arc::clone(&state);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                serve(stream, &serving);
            }
        });
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], port)),
            state,
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn posts(&self) -> Vec<Value> {
        self.state.lock().unwrap().posts.clone()
    }

    fn deleted(&self) -> Vec<String> {
        self.state.lock().unwrap().deleted.clone()
    }

    fn run(&self, args: &[&str]) -> Output {
        main_binary()
            .args(args)
            .env("PENWISE_URL", self.url())
            .env_remove("PENWISE_PORT")
            .output()
            .unwrap()
    }
}

fn bind() -> (TcpListener, u16) {
    for port in FIRST_PORT..=LAST_PORT {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            return (listener, port);
        }
    }
    panic!("no free port in {FIRST_PORT}–{LAST_PORT}");
}

/// A port that accepts and immediately closes: the CLI sees a dead server, and the port stays reserved for the
/// rest of the run (no other test can bind it after this).
fn closed_port() -> u16 {
    let (listener, port) = bind();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            drop(stream);
        }
    });
    port
}

fn serve(mut stream: TcpStream, state: &Arc<Mutex<State>>) {
    let Some(request) = read_request(&mut stream) else {
        return;
    };
    let (status, body) = route(&request, state);
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
         connection: close\r\n\r\n{body}",
        reason(status),
        body.len()
    );
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Error",
    }
}

fn route(request: &Request, state: &Arc<Mutex<State>>) -> (u16, String) {
    if (request.method.as_str(), request.path.as_str()) == ("GET", "/pair/local") {
        return (200, json!({ "token": TOKEN }).to_string());
    }
    if request.headers.get("x-penwise-token").map(String::as_str) != Some(TOKEN) {
        return (401, json!({ "error": "unauthorized" }).to_string());
    }
    let mut state = state.lock().unwrap();
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/projects") => (200, Value::Array(state.projects.clone()).to_string()),
        ("POST", "/projects") => create(request, &mut state),
        ("DELETE", path) if path.starts_with("/projects/") => delete(path, &mut state),
        _ => (404, json!({ "error": "no such route" }).to_string()),
    }
}

fn create(request: &Request, state: &mut State) -> (u16, String) {
    let Ok(body) = serde_json::from_str::<Value>(&request.body) else {
        return (400, json!({ "error": "invalid JSON" }).to_string());
    };
    state.posts.push(body.clone());
    let root = body["root"].as_str().unwrap_or_default().to_owned();
    if let Some(existing) = state.projects.iter().find(|p| p["root"] == root) {
        return (200, existing.to_string());
    }
    let name = body["name"].as_str().unwrap_or("project").to_owned();
    let mut project = json!({
        "id": slug(&name),
        "name": name,
        "root": root,
        "kind": if body.get("static").is_some() { "static" } else { "proxy" },
        "port": PROJECT_PORT,
        "notebook": "nb_test",
    });
    if let Some(folder) = body.get("static") {
        project["static"] = folder.clone();
    }
    if let Some(origin) = body.get("origin") {
        project["origin"] = origin.clone();
    }
    state.projects.push(project.clone());
    (200, project.to_string())
}

fn delete(path: &str, state: &mut State) -> (u16, String) {
    let id = path.trim_start_matches("/projects/").to_owned();
    let before = state.projects.len();
    state
        .projects
        .retain(|project| project["id"] != id.as_str());
    state.deleted.push(id);
    if state.projects.len() == before {
        (404, json!({ "error": "unknown project" }).to_string())
    } else {
        (200, "{}".to_owned())
    }
}

fn slug(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    slug.trim_matches('-').to_owned()
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut raw = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        if let Some(head_end) = find(&raw, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
            let body_start = head_end + 4;
            let wanted = body_start + content_length(&head);
            while raw.len() < wanted {
                match stream.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => raw.extend_from_slice(&buffer[..read]),
                }
            }
            return parse(&head, &raw[body_start..]);
        }
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return None,
            Ok(read) => raw.extend_from_slice(&buffer[..read]),
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn content_length(head: &str) -> usize {
    head.lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn parse(head: &str, body: &[u8]) -> Option<Request> {
    let mut lines = head.lines();
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_owned();
    let path = request_line.next()?.to_owned();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    Some(Request {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(body).into_owned(),
    })
}

// ---------------------------------------------------------------- fixtures

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A self-deleting temp folder; the project lives in a subfolder so its name is stable.
struct Temp {
    path: PathBuf,
}

impl Temp {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "penwise-cli-{label}-{}-{nanos}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn project(&self, name: &str) -> PathBuf {
        let dir = self.path.join(name);
        fs::create_dir(&dir).unwrap();
        dir
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn link_file(dir: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(dir.join(".penwise.json")).unwrap()).unwrap()
}

fn canonical(dir: &Path) -> String {
    dir.canonicalize().unwrap().to_string_lossy().into_owned()
}

// ---------------------------------------------------------------- tests

#[test]
fn link_registers_a_static_site_and_writes_the_link_file() {
    let fake = Fake::start();
    let temp = Temp::new("static");
    let site = temp.project("site");
    fs::write(
        site.join("index.html"),
        "<!doctype html><title>site</title>",
    )
    .unwrap();

    let output = fake.run(&["link", site.to_str().unwrap(), "--yes"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let printed = stdout(&output);
    assert!(
        printed.contains(&format!("http://127.0.0.1:{PROJECT_PORT}/")),
        "{printed}"
    );

    let root = canonical(&site);
    let posts = fake.posts();
    assert_eq!(1, posts.len());
    assert_eq!("site", posts[0]["name"]);
    assert_eq!(root, posts[0]["root"]);
    assert_eq!(root, posts[0]["static"]);
    assert!(posts[0].get("origin").is_none());

    let link = link_file(&site);
    assert_eq!("site", link["project"]);
    assert_eq!("site", link["name"]);
    assert_eq!(root, link["static"]);
    assert!(link.get("origin").is_none());

    let status = fake.run(&["status"]);
    assert!(status.status.success(), "{}", stderr(&status));
    let listed = stdout(&status);
    assert!(listed.contains("site"), "{listed}");
    assert!(listed.contains("static"), "{listed}");
    assert!(listed.contains(&PROJECT_PORT.to_string()), "{listed}");
}

#[test]
fn link_detects_a_vite_dev_server_and_records_its_origin() {
    let fake = Fake::start();
    let temp = Temp::new("vite");
    let app = temp.project("app");
    fs::write(
        app.join("package.json"),
        r#"{"name":"app","scripts":{"dev":"vite"},"devDependencies":{"vite":"^6.0.0"}}"#,
    )
    .unwrap();
    fs::write(
        app.join("vite.config.ts"),
        "export default { server: { port: 5201 } };\n",
    )
    .unwrap();

    let output = fake.run(&["link", app.to_str().unwrap(), "--yes"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("vite dev server at http://127.0.0.1:5201"));

    let posts = fake.posts();
    assert_eq!("http://127.0.0.1:5201", posts[0]["origin"]);
    assert!(posts[0].get("static").is_none());
    assert_eq!("app", posts[0]["name"]);

    let link = link_file(&app);
    assert_eq!("app", link["project"]);
    assert_eq!("http://127.0.0.1:5201", link["origin"]);
    assert!(link.get("static").is_none());

    let status = fake.run(&["status"]);
    assert!(status.status.success(), "{}", stderr(&status));
    // The fake never starts a dev server on 5201, so the probe reports it down.
    assert!(
        stdout(&status).contains("http://127.0.0.1:5201 down"),
        "{}",
        stdout(&status)
    );
}

#[test]
fn relinking_the_same_folder_is_idempotent() {
    let fake = Fake::start();
    let temp = Temp::new("relink");
    let site = temp.project("site");
    fs::write(site.join("index.html"), "<!doctype html>").unwrap();

    for _ in 0..2 {
        let output = fake.run(&["link", site.to_str().unwrap(), "--yes"]);
        assert!(output.status.success(), "{}", stderr(&output));
    }
    assert_eq!(2, fake.posts().len());
    assert_eq!("site", link_file(&site)["project"]);
    let listed = fake.run(&["status"]);
    assert!(
        stdout(&listed).contains("1 project(s)"),
        "{}",
        stdout(&listed)
    );
}

#[test]
fn unlink_deletes_the_registry_entry_and_the_file() {
    let fake = Fake::start();
    let temp = Temp::new("unlink");
    let site = temp.project("site");
    fs::write(site.join("index.html"), "<!doctype html>").unwrap();
    assert!(
        fake.run(&["link", site.to_str().unwrap(), "--yes"])
            .status
            .success()
    );

    let output = fake.run(&["unlink", site.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("unlinked \"site\""));
    assert_eq!(vec!["site".to_owned()], fake.deleted());
    assert!(!site.join(".penwise.json").exists());

    let again = fake.run(&["unlink", site.to_str().unwrap()]);
    assert!(!again.status.success());
    assert!(
        stderr(&again).contains("no .penwise.json"),
        "{}",
        stderr(&again)
    );
}

#[test]
fn link_without_a_detectable_project_explains_the_search() {
    let fake = Fake::start();
    let temp = Temp::new("empty");
    let empty = temp.project("empty");

    let output = fake.run(&["link", empty.to_str().unwrap(), "--yes"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("index.html"),
        "{}",
        stderr(&output)
    );
    assert!(fake.posts().is_empty());
}

#[test]
fn end_of_input_takes_the_default_which_is_yes() {
    let fake = Fake::start();
    let temp = Temp::new("default-yes");
    let site = temp.project("site");
    fs::write(site.join("index.html"), "<!doctype html>").unwrap();

    let output = main_binary()
        .args(["link", site.to_str().unwrap()])
        .env("PENWISE_URL", fake.url())
        .env_remove("PENWISE_PORT")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("register with Penwise? [Y/n]"));
    assert_eq!(1, fake.posts().len());
}

#[test]
fn an_unreachable_server_names_how_to_start_it() {
    let temp = Temp::new("down");
    let site = temp.project("site");
    fs::write(site.join("index.html"), "<!doctype html>").unwrap();
    let port = closed_port();

    let output = main_binary()
        .args(["link", site.to_str().unwrap(), "--yes"])
        .env("PENWISE_URL", format!("http://127.0.0.1:{port}"))
        .env_remove("PENWISE_PORT")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(
        message.contains("cannot reach the Penwise server"),
        "{message}"
    );
    assert!(
        message.contains("systemctl --user start penwise.service"),
        "{message}"
    );
    assert!(!site.join(".penwise.json").exists());
}

// ---------------------------------------------------------------- agent files: hook, skill, unlink

/// `fake.run` with `PATH` set to `path_dir`, so the hook command does not depend on this machine's installation.
fn run_with_path(fake: &Fake, path_dir: &Path, args: &[&str]) -> Output {
    main_binary()
        .args(args)
        .env("PENWISE_URL", fake.url())
        .env_remove("PENWISE_PORT")
        .env("PATH", path_dir)
        .output()
        .unwrap()
}

fn files_under(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(
                    path.strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    out.sort();
    out
}

fn settings(site: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(site.join(".claude/settings.json")).unwrap()).unwrap()
}

fn inbox_commands(settings: &Value) -> Vec<String> {
    settings["hooks"]["UserPromptSubmit"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|group| group["hooks"].as_array().unwrap().clone())
        .filter_map(|hook| hook["command"].as_str().map(str::to_owned))
        .filter(|command| command.ends_with(" inbox"))
        .collect()
}

#[test]
fn a_fresh_project_gets_the_hook_and_both_skills_and_unlink_removes_exactly_them() {
    let fake = Fake::start();
    let temp = Temp::new("agent-fresh");
    let site = temp.project("site");
    fs::write(site.join("index.html"), "<!doctype html>").unwrap();
    let empty_path = temp.project("empty-path");
    let before = files_under(&site);

    let output = run_with_path(
        &fake,
        &empty_path,
        &["link", site.to_str().unwrap(), "--yes"],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("Claude Code hook"), "{text}");
    assert!(text.contains("+++"), "the diff is shown: {text}");

    // `penwise` is not on this PATH, so the hook runs the binary that linked, by absolute path.
    let commands = inbox_commands(&settings(&site));
    assert_eq!(1, commands.len());
    assert!(
        commands[0].starts_with('"')
            && commands[0].contains("penwise")
            && commands[0].ends_with("\" inbox"),
        "{commands:?}"
    );
    for agent_dir in [".claude", ".omp"] {
        let skill =
            fs::read_to_string(site.join(agent_dir).join("skills/penwise/SKILL.md")).unwrap();
        assert!(skill.starts_with("---\nname: penwise\n"), "{agent_dir}");
        assert!(skill.lines().count() < 120);
    }
    let record = link_file(&site);
    assert_eq!(record["installed"]["hook"], true);
    assert_eq!(record["installed"]["hook_file_created"], true);
    assert_eq!(record["installed"]["claude_skill"], true);
    assert_eq!(record["installed"]["omp_skill"], true);
    assert_eq!(record["installed"]["mcp"], true);

    let unlinked = run_with_path(&fake, &empty_path, &["unlink", site.to_str().unwrap()]);
    assert!(unlinked.status.success(), "{}", stderr(&unlinked));
    assert_eq!(
        before,
        files_under(&site),
        "the folder is as it was before link: no hook, no skills, no MCP config, no record"
    );
    assert!(!site.join(".claude").exists() && !site.join(".omp").exists());
}

#[test]
fn an_existing_settings_file_is_merged_relinking_adds_nothing_and_unlink_restores_it() {
    let fake = Fake::start();
    let temp = Temp::new("agent-merge");
    let site = temp.project("site");
    fs::write(site.join("index.html"), "<!doctype html>").unwrap();
    // A `penwise` on PATH: the hook is the portable command.
    let bin = temp.project("bin");
    fs::write(bin.join("penwise"), "#!/bin/sh\n").unwrap();
    let original = json!({
        "model": "opus",
        "permissions": { "allow": ["Bash(ls)"] },
        "hooks": {
            "Stop": [{ "hooks": [{ "type": "command", "command": "say done" }] }],
            "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "date >> /tmp/prompts.log" }] }]
        }
    });
    fs::create_dir_all(site.join(".claude")).unwrap();
    fs::write(site.join(".claude/settings.json"), original.to_string()).unwrap();

    for _ in 0..2 {
        let output = run_with_path(&fake, &bin, &["link", site.to_str().unwrap(), "--yes"]);
        assert!(output.status.success(), "{}", stderr(&output));
    }
    let merged = settings(&site);
    assert_eq!(merged["model"], "opus");
    assert_eq!(merged["hooks"]["Stop"], original["hooks"]["Stop"]);
    assert_eq!(
        merged["hooks"]["UserPromptSubmit"][0], original["hooks"]["UserPromptSubmit"][0],
        "the owner's own hook is untouched and still first"
    );
    assert_eq!(
        vec!["penwise inbox".to_owned()],
        inbox_commands(&merged),
        "linking twice leaves one hook"
    );
    let record = link_file(&site);
    assert_eq!(record["installed"]["hook"], true);
    assert!(
        record["installed"].get("hook_file_created").is_none(),
        "the file was the owner's"
    );

    let unlinked = run_with_path(&fake, &bin, &["unlink", site.to_str().unwrap()]);
    assert!(unlinked.status.success(), "{}", stderr(&unlinked));
    assert_eq!(
        original,
        settings(&site),
        "settings.json is as the owner had it"
    );
    assert!(!site.join(".omp").exists());
    assert!(!site.join(".claude/skills").exists());
}

#[test]
fn unlink_keeps_a_skill_the_owner_edited() {
    let fake = Fake::start();
    let temp = Temp::new("agent-edited");
    let site = temp.project("site");
    fs::write(site.join("index.html"), "<!doctype html>").unwrap();
    let empty_path = temp.project("empty-path");
    assert!(
        run_with_path(
            &fake,
            &empty_path,
            &["link", site.to_str().unwrap(), "--yes"]
        )
        .status
        .success()
    );
    let skill = site.join(".claude/skills/penwise/SKILL.md");
    let mut text = fs::read_to_string(&skill).unwrap();
    text.push_str("\nProject rule: always ask before touching /src/legacy.\n");
    fs::write(&skill, &text).unwrap();

    let output = run_with_path(&fake, &empty_path, &["unlink", site.to_str().unwrap()]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("edited"), "{}", stdout(&output));
    assert_eq!(text, fs::read_to_string(&skill).unwrap());
    assert!(!site.join(".omp").exists(), "the untouched omp skill went");
}

#[test]
fn declining_the_prompts_writes_nothing_beyond_the_link() {
    let fake = Fake::start();
    let temp = Temp::new("agent-decline");
    let site = temp.project("site");
    fs::write(site.join("index.html"), "<!doctype html>").unwrap();
    let mut child = main_binary()
        .args(["link", site.to_str().unwrap()])
        .env("PENWISE_URL", fake.url())
        .env_remove("PENWISE_PORT")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // register: y; MCP config: n; hook: n; skill: n.
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"y\nn\nn\nn\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        vec![".penwise.json".to_owned(), "index.html".to_owned()],
        files_under(&site)
    );
    assert!(link_file(&site).get("installed").is_none());
}
