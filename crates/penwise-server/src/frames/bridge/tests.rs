//! Chromium tests for the injected bridge (contract C3): layout messages on scroll/resize/DOM changes, stable
//! element keys across a re-render, `null` for a removed element, the state recipe and its replay — plus the M1
//! scroll/navigate/route behavior.
//!
//! The fixture is `tests/fixtures/harness/`: a nav menu (`aria-expanded` + `aria-controls`), a `details`, a list
//! the page re-renders by recreating its nodes, and a box a CSS class moves. Chromium is the same headless
//! browser the server uses for `/inspect/*`; when it is not installed these tests report that and pass (the
//! feature needs a browser by definition).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::too_many_lines
)]

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};

use crate::frames::{ProjectConfig, ProjectServer};
use crate::inspect::{Inspect, Page};

fn chromium_installed() -> bool {
    let path = std::env::var_os("PENWISE_CHROMIUM")
        .map_or_else(|| PathBuf::from("/usr/bin/chromium"), PathBuf::from);
    path.is_file()
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/harness")
}

async fn project() -> ProjectServer {
    ProjectServer::start(
        ProjectConfig::static_site("harness", "127.0.0.1", 0, fixture()),
        "bridge-token".to_owned(),
    )
    .await
    .expect("project port")
}

/// Opens the fixture and records every bridge message the page posts to itself.
async fn open(server: &ProjectServer) -> (Inspect, Page) {
    let inspect = Inspect::new();
    let page = inspect
        .open(&format!("http://{}/", server.addr), 480, 800)
        .await
        .expect("open");
    inspect
        .eval(
            &page,
            "window.__pw = []; window.addEventListener('message', function (event) { \
               if (event.data && event.data.t) window.__pw.push(event.data); }); 1",
        )
        .await
        .expect("recorder");
    (inspect, page)
}

async fn messages(inspect: &Inspect, page: &Page, kind: &str) -> Vec<Value> {
    let value = inspect
        .eval(
            page,
            &format!(
                "window.__pw.filter(function (m) {{ return m.t === {}; }})",
                serde_json::to_string(kind).expect("kind json")
            ),
        )
        .await
        .expect("messages");
    value.as_array().cloned().unwrap_or_default()
}

/// The newest message of `kind` that `matches` accepts, polling for up to 6 s.
async fn wait_message(
    inspect: &Inspect,
    page: &Page,
    kind: &str,
    matches: impl Fn(&Value) -> bool,
) -> Value {
    for _ in 0..120 {
        let all = messages(inspect, page, kind).await;
        if let Some(found) = all.iter().rev().find(|message| matches(message)) {
            return found.clone();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let all = messages(inspect, page, kind).await;
    panic!("no matching {kind}; last: {:?}", all.last());
}

async fn wait_layout(inspect: &Inspect, page: &Page, matches: impl Fn(&Value) -> bool) -> Value {
    wait_layout_since(inspect, page, 0, matches).await
}

/// Like [`wait_layout`], but only messages at or after index `since` count (so a repeated value, e.g. an empty
/// state, must arrive *again* for the action under test).
async fn wait_layout_since(
    inspect: &Inspect,
    page: &Page,
    since: usize,
    matches: impl Fn(&Value) -> bool,
) -> Value {
    for _ in 0..120 {
        let all = messages(inspect, page, "pw:layout").await;
        if let Some(found) = all
            .iter()
            .enumerate()
            .rev()
            .find(|(index, message)| *index >= since && matches(message))
            .map(|(_, message)| message)
        {
            return found.clone();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let all = messages(inspect, page, "pw:layout").await;
    panic!("no matching pw:layout; last: {:?}", all.last());
}

async fn layout_count(inspect: &Inspect, page: &Page) -> usize {
    messages(inspect, page, "pw:layout").await.len()
}

async fn watch(inspect: &Inspect, page: &Page, keys: &[String]) {
    let keys = serde_json::to_string(keys).expect("keys json");
    inspect
        .eval(
            page,
            &format!("window.PenwiseBridge.receive({{t:'pw:watch', keys:{keys}}}); 1"),
        )
        .await
        .expect("watch");
}

/// Hits the center of `selector` through the bridge and returns the stable element key of the answer.
async fn hit_key(inspect: &Inspect, page: &Page, selector: &str) -> String {
    let selector = serde_json::to_string(selector).expect("selector json");
    let point = inspect
        .eval(
            page,
            &format!(
                "(function () {{ var e = document.querySelector({selector}); \
                   e.scrollIntoView({{block: 'center'}}); var r = e.getBoundingClientRect(); \
                   return [r.left + r.width / 2, r.top + r.height / 2]; }})()"
            ),
        )
        .await
        .expect("point");
    let x = point[0].as_f64().expect("x");
    let y = point[1].as_f64().expect("y");
    let before = messages(inspect, page, "pw:hit").await.len();
    inspect
        .eval(
            page,
            &format!("window.PenwiseBridge.receive({{t:'pw:hit?', id:'probe', x:{x}, y:{y}}}); 1"),
        )
        .await
        .expect("hit");
    for _ in 0..120 {
        let hits = messages(inspect, page, "pw:hit").await;
        if hits.len() > before
            && let Some(answer) = hits.last()
            && let Some(key) = answer["key"].as_str()
        {
            return key.to_owned();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no pw:hit answer for {selector}");
}

fn rect_of(message: &Value, key: &str) -> Option<Vec<f64>> {
    let rect = message["rects"].get(key)?;
    let values = rect.as_array()?;
    values.iter().map(Value::as_f64).collect()
}

#[tokio::test]
async fn scroll_position_survives_a_reload() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge scroll test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;

    let scrolled = inspect
        .eval(&page, "window.scrollTo(0, 600); window.scrollY")
        .await
        .expect("scroll");
    assert_eq!(scrolled.as_f64(), Some(600.0));

    inspect
        .eval(&page, "location.reload(); 1")
        .await
        .expect("reload");
    let mut last = Value::Null;
    for _ in 0..100 {
        if let Ok(value) = inspect.eval(&page, "window.scrollY").await
            && value.as_f64().is_some_and(|y| y > 500.0)
        {
            last = value;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        last.as_f64().is_some_and(|y| y > 500.0),
        "scroll restored: {last:?}"
    );

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
    let server = project().await;
    let (inspect, page) = open(&server).await;

    inspect
        .eval(&page, "history.pushState({}, '', '/spa?a=1'); 1")
        .await
        .expect("pushState");
    wait_message(&inspect, &page, "pw:route", |message| {
        message["route"] == json!("/spa?a=1")
    })
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
    for _ in 0..100 {
        if inspect
            .eval(&page, "location.pathname")
            .await
            .expect("path")
            == "/two.html"
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        inspect
            .eval(&page, "location.pathname")
            .await
            .expect("path"),
        Value::from("/two.html")
    );

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

#[tokio::test]
async fn layout_reports_scroll_resize_moves_and_removals() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge layout test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;
    let key = hit_key(&inspect, &page, "#mover").await;
    assert!(key.starts_with("v1||"), "key format: {key}");
    watch(&inspect, &page, std::slice::from_ref(&key)).await;

    let first = wait_layout(&inspect, &page, |message| rect_of(message, &key).is_some()).await;
    let first_rect = rect_of(&first, &key).expect("first rect");

    // A CSS change (a class that moves the box) reaches the layout through the MutationObserver.
    inspect
        .eval(
            &page,
            "document.getElementById('mover').classList.add('moved'); 1",
        )
        .await
        .expect("move");
    let moved = wait_layout(&inspect, &page, |message| {
        rect_of(message, &key).is_some_and(|rect| rect[0] > first_rect[0] + 40.0)
    })
    .await;
    let moved_rect = rect_of(&moved, &key).expect("moved rect");
    assert!(moved_rect[0] > first_rect[0] + 40.0, "moved right");
    assert!(moved_rect[1] > first_rect[1] + 30.0, "moved down");

    // Scroll: the layout carries the position and the viewport-relative rect moves with it.
    inspect
        .eval(&page, "window.scrollTo(0, 300); 1")
        .await
        .expect("scroll");
    let scrolled = wait_layout(&inspect, &page, |message| {
        message["scroll"][1].as_f64().is_some_and(|y| y > 299.0) && rect_of(message, &key).is_some()
    })
    .await;
    let scrolled_rect = rect_of(&scrolled, &key).expect("scrolled rect");
    assert!(
        (moved_rect[1] - scrolled_rect[1] - 300.0).abs() < 1.0,
        "rect follows the scroll: {} -> {}",
        moved_rect[1],
        scrolled_rect[1]
    );

    // Resize: the new viewport arrives in full.
    inspect
        .eval(&page, "window.scrollTo(0, 0); 1")
        .await
        .expect("scroll back");
    page.resize(360, 720).await.expect("resize");
    let resized = wait_layout(&inspect, &page, |message| {
        message["viewport"] == json!([360, 720])
    })
    .await;
    assert_eq!(resized["viewport"], json!([360, 720]));

    // Removing the element sends `null` for the watched key.
    inspect
        .eval(&page, "document.getElementById('mover').remove(); 1")
        .await
        .expect("remove");
    wait_layout(&inspect, &page, |message| {
        message["rects"].get(&key).is_some_and(Value::is_null)
    })
    .await;

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

#[tokio::test]
async fn keys_survive_a_re_render_that_recreates_the_nodes() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge key test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;
    // A `data-pw-src` from an adapter is part of the key, and the fingerprint still anchors it.
    let sourced = hit_key(&inspect, &page, "#title").await;
    assert!(
        sourced.starts_with("v1|index.html:10:5|"),
        "sourced key {sourced}"
    );

    let key = hit_key(&inspect, &page, "#list li:nth-of-type(2)").await;

    inspect
        .eval(&page, "window.pwHarness.rerender(); 1")
        .await
        .expect("re-render");
    let again = hit_key(&inspect, &page, "#list li:nth-of-type(2)").await;
    assert_eq!(again, key, "the key follows the recreated node");

    // The watched key must now track the new node: moving it changes its rect.
    watch(&inspect, &page, std::slice::from_ref(&key)).await;
    let before = wait_layout(&inspect, &page, |message| rect_of(message, &key).is_some()).await;
    let left = rect_of(&before, &key).expect("rect before")[0];
    inspect
        .eval(
            &page,
            "document.querySelectorAll('#list li')[1].style.marginLeft = '30px'; 1",
        )
        .await
        .expect("move the new row");
    let moved = wait_layout(&inspect, &page, |message| {
        rect_of(message, &key).is_some_and(|rect| rect[0] >= left + 29.0)
    })
    .await;
    assert!(rect_of(&moved, &key).expect("moved rect")[0] >= left + 29.0);

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

#[tokio::test]
async fn state_recipe_records_the_menu_open_resets_on_close_and_replays() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge state test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;

    inspect
        .eval(&page, "document.getElementById('menu-button').click(); 1")
        .await
        .expect("open the menu");
    let opened = wait_layout(&inspect, &page, |message| {
        message["state"] == json!("click:#menu-button")
    })
    .await;
    assert_eq!(opened["state"], json!("click:#menu-button"));
    assert_eq!(
        inspect
            .eval(&page, "document.getElementById('menu').hidden")
            .await
            .expect("hidden"),
        Value::Bool(false)
    );

    let before_close = layout_count(&inspect, &page).await;
    inspect
        .eval(&page, "document.getElementById('menu-button').click(); 1")
        .await
        .expect("close the menu");
    wait_layout_since(&inspect, &page, before_close, |message| {
        message["state"] == json!("")
    })
    .await;
    assert_eq!(
        inspect
            .eval(&page, "document.getElementById('menu').hidden")
            .await
            .expect("hidden"),
        Value::Bool(true)
    );

    inspect
        .eval(
            &page,
            "window.PenwiseBridge.receive({t:'pw:replay', state:'click:#menu-button'}); 1",
        )
        .await
        .expect("replay");
    let replayed = wait_message(&inspect, &page, "pw:replayed", |message| {
        message["ok"] == json!(true)
    })
    .await;
    assert_eq!(replayed["state"], json!("click:#menu-button"));
    assert_eq!(
        inspect
            .eval(&page, "document.getElementById('menu').hidden")
            .await
            .expect("hidden"),
        Value::Bool(false),
        "replay reopens the menu"
    );

    // Leave the replayed menu so the details toggle starts from the initial state. The state can pass through
    // the recipe and back to "" between two layout frames (the bridge reports the latest snapshot, not every
    // transient), so only the DOM close is waited for here.
    inspect
        .eval(&page, "document.getElementById('menu-button').click(); 1")
        .await
        .expect("close the replayed menu");
    let mut hidden = Value::Null;
    for _ in 0..100 {
        hidden = inspect
            .eval(&page, "document.getElementById('menu').hidden")
            .await
            .expect("hidden");
        if hidden == Value::Bool(true) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(hidden, Value::Bool(true), "the replayed menu closes");

    // The details toggle records its summary selector and replays the same way.
    inspect
        .eval(&page, "document.querySelector('#more summary').click(); 1")
        .await
        .expect("open the details");
    let details = wait_layout(&inspect, &page, |message| {
        message["state"]
            .as_str()
            .is_some_and(|state| state.starts_with("click:") && state.contains("summary"))
    })
    .await;
    let recipe = details["state"].as_str().expect("recipe").to_owned();
    let before_details_close = layout_count(&inspect, &page).await;
    inspect
        .eval(&page, "document.querySelector('#more summary').click(); 1")
        .await
        .expect("close the details");
    wait_layout_since(&inspect, &page, before_details_close, |message| {
        message["state"] == json!("")
    })
    .await;

    let before_details_replay = layout_count(&inspect, &page).await;
    inspect
        .eval(
            &page,
            &format!(
                "window.PenwiseBridge.receive({{t:'pw:replay', state:{}}}); 1",
                serde_json::to_string(&recipe).expect("recipe json")
            ),
        )
        .await
        .expect("replay the details");
    wait_message(&inspect, &page, "pw:replayed", |message| {
        message["ok"] == json!(true) && message["state"] == json!(recipe)
    })
    .await;
    assert_eq!(
        inspect
            .eval(&page, "document.getElementById('more').open")
            .await
            .expect("details open"),
        Value::Bool(true)
    );
    // The replayed state is reported once before the next change, so the navigation below produces a fresh
    // empty-state message rather than being coalesced with it.
    wait_layout_since(&inspect, &page, before_details_replay, |message| {
        message["state"] == json!(recipe)
    })
    .await;

    // A navigation starts a new recipe.
    let before_navigation = layout_count(&inspect, &page).await;
    inspect
        .eval(&page, "history.pushState({}, '', '/elsewhere'); 1")
        .await
        .expect("pushState");
    wait_layout_since(&inspect, &page, before_navigation, |message| {
        message["state"] == json!("")
    })
    .await;

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}
