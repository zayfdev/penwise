//! `penwise inbox` against the real binary: the line when the server has one, nothing when it has none, and
//! nothing (exit 0, fast) when the server is down or the folder is not linked.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

/// A listener on a port the OS picked, so parallel tests never share one (and never touch 7420-7430).
fn bind() -> (TcpListener, u16) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

/// A server that answers `GET /projects/prj_site/inbox` with `body`, and records every request line.
fn serve(body: &'static str) -> (u16, std::sync::mpsc::Receiver<String>) {
    let (listener, port) = bind();
    let (sent, seen) = std::sync::mpsc::channel();
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buffer = [0_u8; 2048];
            let read = stream.read(&mut buffer).unwrap_or(0);
            let text = String::from_utf8_lossy(&buffer[..read]).into_owned();
            let _ = sent.send(text.lines().next().unwrap_or("").to_owned());
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    (port, seen)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("penwise-inbox-it-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn inbox(dir: &PathBuf, port: u16, project: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_penwise"));
    command
        .arg("inbox")
        .current_dir(dir)
        .env("PENWISE_PORT", port.to_string())
        .env_remove("PENWISE_URL")
        .env_remove("PENWISE_PROJECT");
    if let Some(project) = project {
        command.env("PENWISE_PROJECT", project);
    }
    command.output().unwrap()
}

#[test]
fn the_line_of_the_server_is_printed_for_the_linked_project() {
    let (port, seen) =
        serve(r#"{"line":"Penwise: round 3 sent — 4 marks on Desktop / (call look)"}"#);
    let dir = scratch("line");
    fs::write(
        dir.join(".penwise.json"),
        r#"{"project":"prj_site","name":"site"}"#,
    )
    .unwrap();
    let nested = dir.join("src");
    fs::create_dir_all(&nested).unwrap();
    let output = inbox(&nested, port, None);
    assert!(output.status.success());
    assert_eq!(
        "Penwise: round 3 sent \u{2014} 4 marks on Desktop / (call look)\n",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        "GET /projects/prj_site/inbox HTTP/1.1",
        seen.recv_timeout(Duration::from_secs(2)).unwrap()
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_null_line_prints_nothing() {
    let (port, _seen) = serve(r#"{"line":null}"#);
    let dir = scratch("null");
    let output = inbox(&dir, port, Some("prj_site"));
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_down_server_prints_nothing_exits_zero_and_is_quick() {
    // A port nothing listens on: bind, note it, and let the listener go.
    let (listener, port) = bind();
    drop(listener);
    let dir = scratch("down");
    let started = Instant::now();
    let output = inbox(&dir, port, Some("prj_site"));
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "took {:?}",
        started.elapsed()
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_hung_server_is_given_up_on_after_the_timeout() {
    let (listener, port) = bind();
    // Accepts, never answers.
    thread::spawn(move || {
        let held: Vec<_> = listener.incoming().flatten().collect();
        thread::sleep(Duration::from_secs(5));
        drop(held);
    });
    let dir = scratch("hung");
    let started = Instant::now();
    let output = inbox(&dir, port, Some("prj_site"));
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "took {:?}",
        started.elapsed()
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_folder_without_a_link_prints_nothing() {
    let (port, seen) = serve(r#"{"line":"should not be asked"}"#);
    // The test harness's temp folder has no `.penwise.json` above it; PENWISE_PROJECT is unset.
    let dir = scratch("unlinked");
    let output = inbox(&dir, port, None);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(seen.recv_timeout(Duration::from_millis(200)).is_err());
    fs::remove_dir_all(dir).unwrap();
}
