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

/// The `pw:scroll` message that reports scroll `y`.
async fn scroll_at(inspect: &Inspect, page: &Page, y: f64) -> Value {
    wait_message(inspect, page, "pw:scroll", |message| {
        message["y"].as_f64() == Some(y)
    })
    .await
}

#[tokio::test]
async fn a_scroll_after_wheel_input_is_labelled_user_and_after_quiet_page() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge scroll cause test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;

    inspect
        .eval(
            &page,
            "window.dispatchEvent(new WheelEvent('wheel', {deltaY: 120})); window.scrollTo(0, 300); 1",
        )
        .await
        .expect("wheel");
    let user = scroll_at(&inspect, &page, 300.0).await;
    assert_eq!(user["cause"], "user", "{user}");
    assert!(user.get("id").is_none());
    let layout = wait_layout(&inspect, &page, |message| {
        message["scroll"][1].as_f64() == Some(300.0)
    })
    .await;
    assert_eq!(layout["cause"], "user", "{layout}");

    // Input counts for 250 ms (plus the momentum after it): later, the same call is the page's own doing.
    tokio::time::sleep(Duration::from_millis(700)).await;
    inspect
        .eval(&page, "window.scrollTo(0, 340); 1")
        .await
        .expect("programmatic");
    let page_scroll = scroll_at(&inspect, &page, 340.0).await;
    assert_eq!(page_scroll["cause"], "page", "{page_scroll}");

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

#[tokio::test]
async fn pw_scroll_to_is_a_request_that_echoes_its_id_even_when_the_page_is_already_there() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge scroll request test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;

    inspect
        .eval(
            &page,
            "document.documentElement.style.scrollBehavior = 'smooth'; \
             window.PenwiseBridge.receive({t: 'pw:scrollTo', x: 0, y: 420, id: 7}); window.scrollY",
        )
        .await
        .expect("request");
    let answered = scroll_at(&inspect, &page, 420.0).await;
    assert_eq!(answered["cause"], "request", "{answered}");
    assert_eq!(answered["id"], 7);
    // `instant` means the scroll had landed by the time the call returned, smooth-scroll CSS or not.
    let at = inspect.eval(&page, "window.scrollY").await.expect("y");
    assert_eq!(at.as_f64(), Some(420.0));

    // Asking for where the page already is still gets an answer, or the host would wait for it forever.
    let before = messages(&inspect, &page, "pw:scroll").await.len();
    inspect
        .eval(
            &page,
            "window.PenwiseBridge.receive({t: 'pw:scrollTo', x: 0, y: 420, id: 8}); 1",
        )
        .await
        .expect("again");
    let mut echoed = Value::Null;
    for _ in 0..100 {
        let all = messages(&inspect, &page, "pw:scroll").await;
        if let Some(found) = all.iter().skip(before).find(|m| m["id"] == 8) {
            echoed = found.clone();
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(echoed["cause"], "request", "{echoed}");

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

#[tokio::test]
async fn scroll_into_view_and_a_smooth_scroll_lerp_are_the_pages_own() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge scroll page-cause test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;

    inspect
        .eval(
            &page,
            "var far = document.createElement('div'); far.style.cssText = \
             'position:absolute;top:1500px;height:10px;width:10px'; document.body.appendChild(far); \
             far.scrollIntoView({block: 'start', behavior: 'instant'}); window.scrollY",
        )
        .await
        .expect("scrollIntoView");
    let landed = inspect.eval(&page, "window.scrollY").await.expect("y");
    let y = landed.as_f64().expect("scroll position");
    let into_view = scroll_at(&inspect, &page, y).await;
    assert_eq!(into_view["cause"], "page", "{into_view}");

    // A library's lerp: a requestAnimationFrame loop nudging the scroll position.
    inspect
        .eval(
            &page,
            "new Promise(function (done) { var y = 1000; function step() { y += 9; \
               window.scrollTo(0, y); if (y < 1180) requestAnimationFrame(step); else done(1); } step(); })",
        )
        .await
        .expect("lerp");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let lerp: Vec<Value> = messages(&inspect, &page, "pw:scroll")
        .await
        .into_iter()
        .filter(|m| m["y"].as_f64().is_some_and(|y| y >= 1000.0))
        .collect();
    assert!(!lerp.is_empty());
    assert!(
        lerp.iter().all(|m| m["cause"] == "page"),
        "no input touched the page: {lerp:?}"
    );

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

/// The reload socket outlives an outage: after far more than five failed reconnects (a Wi-Fi drop, a server
/// restart) it still reconnects and reloads on the next change, backs off to the cap, retries at once on
/// `online`, and never holds two sockets. A socket that never worked (a proxied dev server) still gives up.
#[tokio::test]
async fn the_reload_socket_survives_a_long_outage() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge reload retry test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;
    let script = r#"(function () {
      function rig() {
        var r = { made: [], timers: [], reloads: 0 };
        function Fake(url) { r.made.push(this); }
        r.env = {
          WebSocket: Fake, random: function () { return 0; },
          setTimeout: function (f, ms) { r.timers.push({ f: f, ms: ms }); return r.timers.length; },
          clearTimeout: function (id) { if (r.timers[id - 1]) r.timers[id - 1].f = null; },
          reload: function () { r.reloads += 1; }
        };
        r.last = function () { return r.made[r.made.length - 1]; };
        r.fail = function () {
          r.last().onclose();
          var t = r.timers[r.timers.length - 1];
          var f = t.f; t.f = null;
          return { ms: t.ms, run: f };
        };
        return r;
      }
      var out = {};
      var up = rig();
      window.PenwiseBridge.liveReload(up.env);
      up.last().onopen();
      out.delays = [];
      for (var i = 0; i < 9; i++) {
        var step = up.fail();
        out.delays.push(step.ms);
        step.run();
      }
      out.sockets = up.made.length;
      up.last().onopen();
      up.last().onmessage({ data: JSON.stringify({ t: "pw:reload" }) });
      out.reloads = up.reloads;
      up.fail();
      var before = up.made.length;
      window.dispatchEvent(new Event("online"));
      out.onlineSockets = up.made.length - before;
      window.dispatchEvent(new Event("online"));
      out.doubleOnline = up.made.length - before;
      var cold = rig();
      window.PenwiseBridge.liveReload(cold.env);
      var stopped = false;
      for (var j = 0; j < 8 && !stopped; j++) {
        cold.last().onclose();
        var t = cold.timers[cold.timers.length - 1];
        if (!t || !t.f) { stopped = true; break; }
        var run = t.f; t.f = null; run();
        if (cold.timers.length === j + 1 && cold.made.length === j + 1) stopped = true;
      }
      out.coldSockets = cold.made.length;
      out.curve = [1, 2, 3, 4, 5, 6, 40].map(function (n) { return window.PenwiseBridge.reloadDelay(n, function () { return 0.999; }); });
      return out;
    })()"#;
    let out = inspect.eval(&page, script).await.expect("retry script");
    assert_eq!(
        out["delays"],
        json!([1000, 2000, 4000, 8000, 10000, 10000, 10000, 10000, 10000]),
        "{out}"
    );
    assert_eq!(out["sockets"], json!(10), "one new socket per retry");
    assert_eq!(out["reloads"], json!(1), "a reload message reloads: {out}");
    assert_eq!(
        out["onlineSockets"],
        json!(1),
        "online retries at once: {out}"
    );
    assert_eq!(out["doubleOnline"], json!(1), "never two sockets: {out}");
    assert!(
        out["coldSockets"].as_u64().is_some_and(|n| n <= 6),
        "a socket that never opened gives up: {out}"
    );
    assert_eq!(
        out["curve"],
        json!([1249, 2249, 4249, 8249, 10249, 10249, 10249]),
        "jitter stays under 250 ms and the cap holds: {out}"
    );
}

/// The latest rect the core holds for `key` after every `pw:layout` so far (they merge per key), as a document rect
/// `[x, y, w, h]` (viewport rect + that message's scroll, the core's rule), or `None` when it was never sent.
async fn core_doc_rect(inspect: &Inspect, page: &Page, key: &str) -> Option<Vec<f64>> {
    let all = messages(inspect, page, "pw:layout").await;
    all.iter().rev().find_map(|message| {
        let rect = rect_of(message, key)?;
        let sx = message["scroll"][0].as_f64()?;
        let sy = message["scroll"][1].as_f64()?;
        Some(vec![rect[0] + sx, rect[1] + sy, rect[2], rect[3]])
    })
}

/// A case-study column that sticks while its section scrolls past (`position: sticky; top: 40px`), as on the
/// owner's portfolio: placed before `#tall`, it returns the document y of the section's top.
async fn add_sticky_case(inspect: &Inspect, page: &Page) -> f64 {
    let top = inspect
        .eval(
            page,
            "window.scrollTo(0, 0); var c = document.createElement('section'); c.id = 'pw-case'; \
             c.style.cssText = 'height:1200px;margin:16px 0;background:#f4f4f4'; \
             var s = document.createElement('div'); s.id = 'pw-side'; \
             s.style.cssText = 'position:sticky;top:40px;width:200px;height:100px;background:#cde'; \
             var p = document.createElement('p'); p.id = 'pw-side-text'; p.textContent = 'sticky words'; \
             s.appendChild(p); c.appendChild(s); document.getElementById('tall').before(c); \
             c.getBoundingClientRect().top + window.scrollY",
        )
        .await
        .expect("sticky case");
    top.as_f64().expect("case top")
}

/// A mark bound to a sticky element stays where it was drawn on the page: the bridge reports the element where the
/// flow puts it, not where sticking holds it on screen, so scrolling past never drags the mark along (owner,
/// 2026-11-02: "the drawings start moving down as well with it until they are out of view").
#[tokio::test]
async fn a_sticky_element_is_reported_where_the_flow_puts_it() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge sticky test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;
    let case_top = add_sticky_case(&inspect, &page).await;
    let key = hit_key(&inspect, &page, "#pw-side-text").await;
    inspect
        .eval(
            &page,
            &format!("window.scrollTo(0, {}); 1", case_top - 100.0),
        )
        .await
        .expect("scroll above the case");
    watch(&inspect, &page, std::slice::from_ref(&key)).await;
    let first = wait_layout(&inspect, &page, |message| rect_of(message, &key).is_some()).await;
    let flow = core_doc_rect(&inspect, &page, &key)
        .await
        .expect("flow rect");
    assert!(
        first["scroll"][1].as_f64().is_some_and(|y| y < case_top),
        "not pinned yet: {first}"
    );

    // Scroll 500 px into the case: the column is pinned 40 px under the viewport top, 460 px below its flow place.
    let before = layout_count(&inspect, &page).await;
    let pinned_scroll = case_top + 500.0;
    inspect
        .eval(&page, &format!("window.scrollTo(0, {pinned_scroll}); 1"))
        .await
        .expect("scroll into the case");
    wait_layout_since(&inspect, &page, before, |message| {
        message["scroll"][1]
            .as_f64()
            .is_some_and(|y| (y - pinned_scroll).abs() < 1.0)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let shown = inspect
        .eval(
            &page,
            "document.getElementById('pw-side').getBoundingClientRect().top",
        )
        .await
        .expect("pinned top");
    assert!(
        shown.as_f64().is_some_and(|top| (top - 40.0).abs() < 1.0),
        "the column is pinned on screen: {shown}"
    );
    let held = core_doc_rect(&inspect, &page, &key).await.expect("rect");
    assert!(
        (held[1] - flow[1]).abs() < 0.5 && (held[0] - flow[0]).abs() < 0.5,
        "the core keeps the flow position {flow:?} while the column is pinned, got {held:?}"
    );

    // A pen-down on the pinned column answers with the same flow rect, so a mark drawn now binds where it is drawn.
    let hit = inspect
        .eval(
            &page,
            "(function () { var r = document.getElementById('pw-side-text').getBoundingClientRect(); \
               return window.PenwiseBridge.hitAt(r.left + 4, r.top + 4); })()",
        )
        .await
        .expect("hit while pinned");
    let hit_top = hit["rect"][1].as_f64().expect("hit rect") + pinned_scroll;
    assert!(
        (hit_top - flow[1]).abs() < 0.5,
        "hit rect at the flow position: {hit}"
    );

    // Measuring leaves no trace: no inline style on the column, and no layout loop at rest.
    let style = inspect
        .eval(
            &page,
            "document.getElementById('pw-side').getAttribute('style')",
        )
        .await
        .expect("style");
    assert!(
        style
            .as_str()
            .is_some_and(|s| s.contains("sticky") && !s.contains("static")),
        "style kept: {style}"
    );
    let resting = layout_count(&inspect, &page).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    let after = layout_count(&inspect, &page).await;
    assert!(
        after <= resting + 1,
        "no layout loop at rest: {resting} -> {after}"
    );

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

/// Adds `html` before `#tall`, scrolled to the top, and returns the core's document rect for the element `selector`
/// inside it once the bridge watches it, plus its key.
async fn watch_new(
    inspect: &Inspect,
    page: &Page,
    html: &str,
    selector: &str,
) -> (String, Vec<f64>) {
    let html = serde_json::to_string(html).expect("html json");
    inspect
        .eval(
            page,
            &format!(
                "window.scrollTo(0, 0); var holder = document.createElement('div'); holder.innerHTML = {html}; \
                 while (holder.firstChild) document.getElementById('tall').before(holder.firstChild); 1"
            ),
        )
        .await
        .expect("insert");
    let key = hit_key(inspect, page, selector).await;
    inspect
        .eval(page, "window.scrollTo(0, 0); 1")
        .await
        .expect("top");
    watch(inspect, page, std::slice::from_ref(&key)).await;
    wait_layout(inspect, page, |message| rect_of(message, &key).is_some()).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let rect = core_doc_rect(inspect, page, &key).await.expect("rect");
    (key, rect)
}

/// The document top of `selector`'s layout box, measured by the test with every transform switched off.
async fn untransformed_top(inspect: &Inspect, page: &Page, selector: &str, ancestors: &str) -> f64 {
    let selector = serde_json::to_string(selector).expect("selector json");
    let ancestors = serde_json::to_string(ancestors).expect("ancestors json");
    inspect
        .eval(
            page,
            &format!(
                "(function () {{ var all = [document.querySelector({selector})].concat( \
                   Array.prototype.slice.call(document.querySelectorAll({ancestors}))); \
                 var kept = all.map(function (e) {{ var t = [e.style.transition, e.style.transform]; \
                   e.style.transition = 'none'; e.style.transform = 'none'; return t; }}); \
                 var top = document.querySelector({selector}).getBoundingClientRect().top + window.scrollY; \
                 all.forEach(function (e, i) {{ e.style.transform = kept[i][1]; }}); \
                 document.body.offsetWidth; \
                 all.forEach(function (e, i) {{ e.style.transition = kept[i][0]; }}); return top; }})()"
            ),
        )
        .await
        .expect("layout top")
        .as_f64()
        .expect("layout top")
}

/// A reveal-on-scroll block starts slid down and slides up into place with a CSS transform transition. The mark
/// bound to it lives at the block's layout position the whole time: before, during and after the slide (owner,
/// 2026-11-02: scrolling never moves a mark). Before, the core first held the slid-down rect and the mark slid.
#[tokio::test]
async fn a_transform_reveal_never_moves_a_watched_rect() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge reveal test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;
    let html = "<div id='pw-slide' style='height:60px;background:#dfd;\
                transition:transform 400ms linear,opacity 400ms linear;transform:translateY(30px);opacity:0'>\
                slides up</div>";
    let (key, first) = watch_new(&inspect, &page, html, "#pw-slide").await;
    let layout = untransformed_top(&inspect, &page, "#pw-slide", "#pw-none").await;
    assert!(
        (first[1] - layout).abs() < 0.5,
        "slid down: held {} for the layout top {layout}",
        first[1]
    );

    inspect
        .eval(
            &page,
            "var b = document.getElementById('pw-slide'); b.style.transform = 'none'; b.style.opacity = '1'; 1",
        )
        .await
        .expect("reveal");
    for wait in [150, 700] {
        tokio::time::sleep(Duration::from_millis(wait)).await;
        let held = core_doc_rect(&inspect, &page, &key).await.expect("rect");
        assert!(
            (held[1] - layout).abs() < 0.5,
            "after {wait} ms: held {} for {layout}",
            held[1]
        );
    }

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

/// A parallax block that page script moves with `transform` on every scroll keeps its mark at its layout position.
/// Under a scaled ancestor (not a pure translation) the layout box comes from the offset chain instead.
#[tokio::test]
async fn parallax_and_scaled_ancestors_never_move_a_watched_rect() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge parallax test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;
    let html = "<div id='pw-zoom' style='transform:scale(0.8) translateY(40px);transform-origin:0 0'>\
                <div id='pw-par' style='height:80px;margin-top:20px;background:#fdd'>parallax</div></div>";
    let (key, first) = watch_new(&inspect, &page, html, "#pw-par").await;
    inspect
        .eval(
            &page,
            "window.addEventListener('scroll', function () { \
               document.getElementById('pw-par').style.transform = 'translateY(' + window.scrollY * 0.4 + 'px)'; \
             }, { passive: true }); 1",
        )
        .await
        .expect("parallax");
    let layout = untransformed_top(&inspect, &page, "#pw-par", "#pw-zoom").await;
    assert!(
        (first[1] - layout).abs() < 1.0,
        "under the scale: held {} for {layout}",
        first[1]
    );
    for y in [150, 400] {
        let before = layout_count(&inspect, &page).await;
        inspect
            .eval(&page, &format!("window.scrollTo(0, {y}); 1"))
            .await
            .expect("scroll");
        wait_layout_since(&inspect, &page, before, |message| {
            message["scroll"][1]
                .as_f64()
                .is_some_and(|s| (s - f64::from(y)).abs() < 1.0)
        })
        .await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let held = core_doc_rect(&inspect, &page, &key).await.expect("rect");
        assert!(
            (held[1] - layout).abs() < 1.0,
            "scrolled to {y}: held {} for {layout}",
            held[1]
        );
    }

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}

/// Binding still carries a mark through a real layout change: content inserted above moves the element down, and
/// the core follows it.
#[tokio::test]
async fn content_inserted_above_moves_a_watched_rect() {
    if !chromium_installed() {
        tracing::warn!("chromium is not installed; bridge layout-change test skipped");
        return;
    }
    let server = project().await;
    let (inspect, page) = open(&server).await;
    let html = "<div id='pw-target' style='height:40px;background:#ddf'>target</div>";
    let (key, first) = watch_new(&inspect, &page, html, "#pw-target").await;
    inspect
        .eval(
            &page,
            "var gap = document.createElement('div'); gap.style.height = '50px'; \
             document.getElementById('pw-target').before(gap); 1",
        )
        .await
        .expect("insert above");
    let moved = wait_layout(&inspect, &page, |message| {
        rect_of(message, &key).is_some_and(|rect| {
            rect[1] + message["scroll"][1].as_f64().unwrap_or(0.0) > first[1] + 49.0
        })
    })
    .await;
    let rect = rect_of(&moved, &key).expect("moved rect");
    let doc = rect[1] + moved["scroll"][1].as_f64().expect("scroll");
    assert!(
        (doc - first[1] - 50.0).abs() < 0.5,
        "moved by the 50 px inserted above: {} -> {doc}",
        first[1]
    );

    page.close().await;
    inspect.shutdown().await;
    server.shutdown().await;
}
