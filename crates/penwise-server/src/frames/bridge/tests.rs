//! Chromium tests for the injected bridge (contract C3): the scroll position survives a reload, `pw:navigate`
//! navigates within the origin, and route changes are reported.
//!
//! Chromium is the same headless browser the server uses for `/inspect/*`; when it is not installed these tests
//! report that and pass (the feature needs a browser by definition).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

use std::path::PathBuf;
use std::time::Duration;

use penwise_testkit::TempDir;
use serde_json::Value;

use crate::frames::{ProjectConfig, ProjectServer};
use crate::inspect::{Inspect, Page};

const PAGE: &str = r#"<!doctype html>
<html>
<head><title>bridge</title><style>body{margin:0}#tall{height:4000px}</style></head>
<body><div id="tall">tall</div><a id="two" href="/two.html">two</a></body>
</html>
"#;
const TWO: &str = "<!doctype html><html><head><title>two</title></head><body>two</body></html>";

fn chromium_installed() -> bool {
    let path = std::env::var_os("PENWISE_CHROMIUM")
        .map_or_else(|| PathBuf::from("/usr/bin/chromium"), PathBuf::from);
    path.is_file()
}

async fn project() -> (TempDir, ProjectServer) {
    let dir = TempDir::new("bridge").expect("tempdir");
    std::fs::write(dir.path().join("index.html"), PAGE).expect("index");
    std::fs::write(dir.path().join("two.html"), TWO).expect("two");
    let server = ProjectServer::start(
        ProjectConfig::static_site("bridge", "127.0.0.1", 0, dir.path()),
        "bridge-token".to_owned(),
    )
    .await
    .expect("project port");
    (dir, server)
}

/// Polls `expression` until `done` accepts its value.
async fn wait_until(
    inspect: &Inspect,
    page: &Page,
    expression: &str,
    done: impl Fn(&Value) -> bool,
) -> Value {
    let mut last = Value::Null;
    for _ in 0..100 {
        if let Ok(value) = inspect.eval(page, expression).await {
            if done(&value) {
                return value;
            }
            last = value;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {expression}: {last:?}");
}

#[tokio::test]
async fn scroll_position_survives_a_reload() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge scroll test skipped");
        return;
    }
    let (_dir, server) = project().await;
    let inspect = Inspect::new();
    let page = inspect
        .open(&format!("http://{}/", server.addr), 400, 800)
        .await
        .expect("open");

    let scrolled = inspect
        .eval(&page, "window.scrollTo(0, 600); window.scrollY")
        .await
        .expect("scroll");
    assert_eq!(scrolled.as_f64(), Some(600.0));

    inspect
        .eval(&page, "location.reload(); 1")
        .await
        .expect("reload");
    wait_until(&inspect, &page, "window.scrollY", |value| {
        value.as_f64().is_some_and(|y| y > 500.0)
    })
    .await;

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

#[tokio::test]
async fn route_changes_are_reported_and_navigate_stays_on_the_origin() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge route test skipped");
        return;
    }
    let (_dir, server) = project().await;
    let inspect = Inspect::new();
    let page = inspect
        .open(&format!("http://{}/", server.addr), 400, 800)
        .await
        .expect("open");

    inspect
        .eval(
            &page,
            "window.__pw=[]; window.addEventListener('message', function (event) { \
               if (event.data && event.data.t) window.__pw.push(event.data); }); 1",
        )
        .await
        .expect("listener");

    inspect
        .eval(&page, "history.pushState({}, '', '/spa?a=1'); 1")
        .await
        .expect("pushState");
    wait_until(
        &inspect,
        &page,
        "window.__pw.some(function (m) { return m.t === 'pw:route' && m.route === '/spa?a=1'; })",
        Value::is_boolean,
    )
    .await;

    // A protocol-relative route would leave the origin: the bridge ignores it.
    inspect
        .eval(
            &page,
            "window.PenwiseBridge.receive({t:'pw:navigate', route:'//evil.example/'}); location.pathname",
        )
        .await
        .expect("ignored navigate");
    assert_eq!(
        inspect
            .eval(&page, "location.pathname")
            .await
            .expect("path"),
        Value::from("/spa")
    );

    inspect
        .eval(
            &page,
            "window.PenwiseBridge.receive({t:'pw:navigate', route:'/two.html'}); 1",
        )
        .await
        .expect("navigate");
    wait_until(&inspect, &page, "location.pathname", |value| {
        value == &Value::from("/two.html")
    })
    .await;

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}
