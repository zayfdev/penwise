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

    // Scroll: the layout carries the position; the element's document rect is unchanged, so its rect is not
    // resent (core keeps `rect + scroll` from when it was reported, which is still right).
    inspect
        .eval(&page, "window.scrollTo(0, 300); 1")
        .await
        .expect("scroll");
    let scrolled = wait_layout(&inspect, &page, |message| {
        message["scroll"][1].as_f64().is_some_and(|y| y > 299.0)
    })
    .await;
    assert!(
        rect_of(&scrolled, &key).is_none(),
        "an unmoved element is not resent after a scroll: {scrolled}"
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

/// A `position: fixed` element keeps its viewport rect while the page scrolls, but core stores document rects
/// (`rect + scroll`), so the bridge must resend the rect after a scroll (it compares document rects).
#[tokio::test]
async fn a_fixed_element_is_resent_after_a_scroll() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge fixed-element test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;
    inspect
        .eval(
            &page,
            "var bar = document.createElement('div'); bar.id = 'pw-fixed'; \
             bar.style.cssText = 'position:fixed;top:12px;left:8px;width:120px;height:24px;z-index:9'; \
             bar.textContent = 'fixed'; document.body.appendChild(bar); window.scrollTo(0, 0); 1",
        )
        .await
        .expect("fixed bar");
    let key = hit_key(&inspect, &page, "#pw-fixed").await;
    watch(&inspect, &page, std::slice::from_ref(&key)).await;
    let first = wait_layout(&inspect, &page, |message| {
        message["scroll"][1].as_f64().is_some_and(|y| y < 1.0) && rect_of(message, &key).is_some()
    })
    .await;
    let first_rect = rect_of(&first, &key).expect("first rect");

    let before = layout_count(&inspect, &page).await;
    inspect
        .eval(&page, "window.scrollTo(0, 250); 1")
        .await
        .expect("scroll");
    let scrolled = wait_layout_since(&inspect, &page, before, |message| {
        message["scroll"][1].as_f64().is_some_and(|y| y > 249.0)
    })
    .await;
    let rect = rect_of(&scrolled, &key).expect(
        "a fixed element's rect is resent after a scroll although its viewport rect is unchanged",
    );
    assert_eq!(rect, first_rect, "the viewport rect itself did not change");

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

/* ------------------------------------------------------------------ patches (M4, P3) */

/// Applies a patch through the exposed API (what the server's headless pages do) and returns `{ok, failed}`.
async fn apply_direct(inspect: &Inspect, page: &Page, patch: Value) -> Value {
    let patch = serde_json::to_string(&patch).expect("patch json");
    inspect
        .eval(
            page,
            &format!("window.PenwiseBridge.applyPatch({patch}, true)"),
        )
        .await
        .expect("apply patch")
}

/// Applies a patch the way a host does (a `pw:patch` message) and waits for the `pw:patched` answer.
async fn send_patch(inspect: &Inspect, page: &Page, patch: Value, on: bool) {
    let message = serde_json::to_string(&json!({ "t": "pw:patch", "patch": patch, "on": on }))
        .expect("message json");
    let before = messages(inspect, page, "pw:patched").await.len();
    inspect
        .eval(page, &format!("window.PenwiseBridge.receive({message}); 1"))
        .await
        .expect("patch message");
    for _ in 0..120 {
        if messages(inspect, page, "pw:patched").await.len() > before {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no pw:patched answer");
}

/// `selector`'s text, or `None` when the element is absent or the navigation destroyed the execution context.
async fn probe_text(inspect: &Inspect, page: &Page, selector: &str) -> Option<String> {
    let script = format!(
        "(function () {{ var e = document.querySelector({}); return e ? e.textContent : null; }})()",
        serde_json::to_string(selector).expect("selector json")
    );
    let value = inspect.eval(page, &script).await.ok()?;
    Some(value.as_str().unwrap_or_default().to_owned())
}

async fn text_at(inspect: &Inspect, page: &Page, selector: &str) -> String {
    probe_text(inspect, page, selector).await.expect("text")
}

/// Polls until `selector`'s text is `want` (up to 6 s), returning the last text seen. A reload swaps the
/// document under the evaluator, so "wait until the new page shows this" is the only race-free check.
async fn wait_text(inspect: &Inspect, page: &Page, selector: &str, want: &str) -> String {
    let mut last = String::new();
    for _ in 0..120 {
        if let Some(text) = probe_text(inspect, page, selector).await {
            last = text;
            if last == want {
                return last;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    last
}

fn css_replace_insert_remove() -> Value {
    json!({
      "css": "#mover { background: rgb(1, 2, 3); }",
      "dom": [
        { "op": "replace", "selector": "#title", "html": "<h1 id=\"title\">Patched</h1>" },
        { "op": "insert_before", "selector": "#mover", "html": "<p id=\"before\">before</p>" },
        { "op": "remove", "selector": "[data-row=\"b\"]" },
        { "op": "set_attr", "selector": "#mover", "name": "title", "value": "hi" }
      ]
    })
}

#[tokio::test]
async fn a_patch_applies_css_replace_insert_and_remove() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge patch test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;

    let result = apply_direct(&inspect, &page, css_replace_insert_remove()).await;
    assert_eq!(result["ok"], json!(true), "applied: {result}");
    assert_eq!(result["failed"], json!([]));
    assert_eq!(text_at(&inspect, &page, "#title").await, "Patched");
    assert_eq!(text_at(&inspect, &page, "#before").await, "before");
    assert_eq!(
        inspect
            .eval(&page, "document.querySelector('[data-row=\"b\"]') === null")
            .await
            .expect("removed"),
        json!(true)
    );
    assert_eq!(
        inspect
            .eval(
                &page,
                "getComputedStyle(document.getElementById('mover')).backgroundColor"
            )
            .await
            .expect("computed style"),
        json!("rgb(1, 2, 3)")
    );
    assert_eq!(
        inspect
            .eval(
                &page,
                "document.getElementById('mover').getAttribute('title')"
            )
            .await
            .expect("attribute"),
        json!("hi")
    );
    assert_eq!(
        inspect
            .eval(
                &page,
                "document.querySelectorAll('style[data-pw-patch]').length"
            )
            .await
            .expect("style"),
        json!(1)
    );

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

#[tokio::test]
async fn reverting_a_patch_restores_the_exact_dom() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge patch revert test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;
    let before = inspect
        .eval(&page, "document.getElementById('app').outerHTML")
        .await
        .expect("outerHTML");
    let before = before.as_str().expect("string").to_owned();

    let patch = json!({
      "css": ".box { outline: 3px solid red }",
      "dom": [
        { "op": "replace", "selector": "#title", "html": "<h1 id=\"title\">Changed</h1>" },
        { "op": "remove", "selector": ".row[data-row=\"a\"]" },
        { "op": "insert_after", "selector": "#mover", "html": "<i id=\"after\">after</i>" },
        { "op": "prepend", "selector": "#list", "html": "<li id=\"first\">first</li>" },
        { "op": "set_text", "selector": "#two", "html": null, "value": "two!" },
        { "op": "add_class", "selector": "#mover", "name": "moved" },
        { "op": "remove_class", "selector": "#title", "name": "gone" },
        { "op": "set_attr", "selector": "#mover", "name": "data-state", "value": "open" },
        { "op": "remove_attr", "selector": "#two", "name": "href" }
      ]
    });
    let result = apply_direct(&inspect, &page, patch).await;
    assert_eq!(result["ok"], json!(true), "applied: {result}");
    assert_ne!(
        inspect
            .eval(&page, "document.getElementById('app').outerHTML")
            .await
            .expect("patched outerHTML"),
        json!(before),
        "the patch changed the DOM"
    );

    let reverted = inspect
        .eval(&page, "window.PenwiseBridge.revertPatch(); 1")
        .await
        .expect("revert");
    assert_eq!(reverted, json!(1));
    let after = inspect
        .eval(&page, "document.getElementById('app').outerHTML")
        .await
        .expect("outerHTML");
    assert_eq!(after.as_str(), Some(before.as_str()), "revert is exact");
    assert_eq!(
        inspect
            .eval(
                &page,
                "document.querySelectorAll('style[data-pw-patch]').length"
            )
            .await
            .expect("style"),
        json!(0)
    );

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

#[tokio::test]
async fn a_patch_is_reapplied_after_a_reload() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge patch reload test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;
    send_patch(
        &inspect,
        &page,
        json!({ "css": "#mover { background: rgb(9, 8, 7); }", "dom": [
            { "op": "set_text", "selector": "#mover", "value": "patched" }
        ] }),
        true,
    )
    .await;
    assert_eq!(text_at(&inspect, &page, "#mover").await, "patched");

    inspect
        .eval(&page, "location.reload(); 1")
        .await
        .expect("reload");
    // The old document says "patched", so seeing "mover" proves the reloaded page is the unpatched one and
    // its bridge is back. Then behave as a host: re-send the patch after the load.
    let reloaded = wait_text(&inspect, &page, "#mover", "mover").await;
    assert_eq!(reloaded, "mover", "the reloaded document is unpatched");
    let result = apply_direct(
        &inspect,
        &page,
        json!({ "css": "#mover { background: rgb(9, 8, 7); }", "dom": [
            { "op": "set_text", "selector": "#mover", "value": "patched" }
        ] }),
    )
    .await;
    assert_eq!(result["ok"], json!(true));
    let patched = wait_text(&inspect, &page, "#mover", "patched").await;
    assert_eq!(patched, "patched", "the host re-applied the patch");

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

#[tokio::test]
async fn failed_selectors_are_reported_and_the_rest_applies() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge patch failure test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;
    let result = apply_direct(
        &inspect,
        &page,
        json!({ "dom": [
            { "op": "set_text", "selector": "#title", "value": "first" },
            { "op": "set_attr", "selector": ".does-not-exist", "name": "title", "value": "x" },
            { "op": "remove", "selector": "div[" }
        ] }),
    )
    .await;
    assert_eq!(result["ok"], json!(false), "{result}");
    let failed = result["failed"].as_array().expect("failed list");
    assert_eq!(failed.len(), 2, "{result}");
    assert_eq!(failed[0]["index"], json!(1));
    assert_eq!(failed[0]["reason"], json!("no match"));
    assert_eq!(failed[1]["index"], json!(2));
    assert_eq!(failed[1]["reason"], json!("bad selector"));
    assert_eq!(
        text_at(&inspect, &page, "#title").await,
        "first",
        "the good op ran"
    );

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

#[tokio::test]
async fn layout_and_keys_work_on_the_patched_dom() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge patched-layout test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;
    let key = "v1||div#mover.box:nth-of-type(1)".to_owned();
    watch(&inspect, &page, std::slice::from_ref(&key)).await;
    let first = wait_layout(&inspect, &page, |message| rect_of(message, &key).is_some()).await;
    let x_before = rect_of(&first, &key).expect("rect")[0];

    let before = layout_count(&inspect, &page).await;
    send_patch(
        &inspect,
        &page,
        json!({ "css": "#mover { margin-left: 60px }", "dom": [
            { "op": "set_text", "selector": "#mover", "value": "moved" }
        ] }),
        true,
    )
    .await;
    let moved = wait_layout_since(&inspect, &page, before, |message| {
        rect_of(message, &key).is_some_and(|rect| rect[0] > x_before + 30.0)
    })
    .await;
    assert!(
        rect_of(&moved, &key).is_some_and(|rect| rect[0] > x_before + 30.0),
        "the css moved the watched element"
    );

    // The key still resolves to the patched element and the hit still names it.
    let resolved = inspect
        .eval(
            &page,
            &format!(
                "(function () {{ var e = window.PenwiseBridge.resolveKey({}); return e ? e.textContent : null; }})()",
                serde_json::to_string(&key).expect("key json")
            ),
        )
        .await
        .expect("resolve");
    assert_eq!(resolved, json!("moved"));
    let hit = hit_key(&inspect, &page, "#mover").await;
    assert_eq!(hit, key);

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}
