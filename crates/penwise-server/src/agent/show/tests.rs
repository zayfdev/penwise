//! `POST /agent/show` through the real project group. None of these need a browser: frames are items, and the
//! camera moves are recorded by the test rig's announcer.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use penwise_core::item::{FrameExtent, ItemId, ItemKind, Layer};
use serde_json::{Value, json};

use crate::api::projects::routes as group;
use crate::testing::Rig;

struct Site {
    base: String,
    project: String,
}

async fn post(url: &str, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(url)
        .json(&body)
        .send()
        .await
        .expect("post")
}

async fn site(rig: &Rig) -> Site {
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("index.html"), "<!doctype html><h1>hi</h1>").unwrap();
    let base = rig.serve(group(Arc::new(String::new()))).await;
    let linked: Value = post(
        &format!("{base}/projects"),
        json!({"name": "site", "root": root, "static": root}),
    )
    .await
    .json()
    .await
    .unwrap();
    Site {
        base,
        project: linked["id"].as_str().unwrap().to_owned(),
    }
}

async fn show(site: &Site, what: Value) -> reqwest::Response {
    let mut body = what;
    body["project"] = json!(site.project);
    post(&format!("{}/agent/show", site.base), body).await
}

async fn ok(site: &Site, what: Value) -> Value {
    let response = show(site, what).await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    response.json().await.unwrap()
}

#[tokio::test]
async fn a_frame_opens_at_desktop_full_length_and_the_next_call_reuses_it() {
    let rig = Rig::new();
    let site = site(&rig).await;
    let first = ok(&site, json!({"frame": {"route": "/"}})).await;
    assert_eq!(first["preset"], "desktop");
    assert_eq!(first["extent"], "full");
    assert_eq!(first["reused"], false);

    let page = first["page"].as_str().unwrap().parse().unwrap();
    let view = rig.view(page).await;
    let frame = ItemId::from_hex(first["frame"].as_str().unwrap()).unwrap();
    let ItemKind::Frame(item) = &view.replica.scene().get(frame).unwrap().kind else {
        panic!("not a frame");
    };
    assert_eq!((item.css_w, item.css_h), (1440, 900));
    assert_eq!(item.extent, FrameExtent::Full);

    let second = ok(&site, json!({"frame": {"route": "/"}})).await;
    assert_eq!(second["reused"], true);
    assert_eq!(second["frame"], first["frame"]);

    let phone = ok(&site, json!({"frame": {"route": "/", "preset": "phone"}})).await;
    assert_eq!(phone["reused"], false, "another preset is another frame");
    assert_ne!(phone["frame"], first["frame"]);
    let other = ok(&site, json!({"frame": {"route": "/work"}})).await;
    assert_eq!(other["reused"], false, "another route is another frame");

    let focused = rig.focused.lock().unwrap().clone();
    assert_eq!(focused.len(), 4, "the owner's view follows every call");
    assert_eq!(focused[0], (page, Some(frame)));
}

#[tokio::test]
async fn a_viewport_extent_is_applied_to_a_new_and_to_an_existing_frame() {
    let rig = Rig::new();
    let site = site(&rig).await;
    let opened = ok(
        &site,
        json!({"frame": {"route": "/", "extent": "viewport"}}),
    )
    .await;
    let page = opened["page"].as_str().unwrap().parse().unwrap();
    let frame = ItemId::from_hex(opened["frame"].as_str().unwrap()).unwrap();
    let view = rig.view(page).await;
    let ItemKind::Frame(item) = &view.replica.scene().get(frame).unwrap().kind else {
        panic!("not a frame");
    };
    assert_eq!(item.extent, FrameExtent::Viewport);

    ok(&site, json!({"frame": {"route": "/", "extent": "full"}})).await;
    let view = rig.view(page).await;
    let ItemKind::Frame(item) = &view.replica.scene().get(frame).unwrap().kind else {
        panic!("not a frame");
    };
    assert_eq!(item.extent, FrameExtent::Full);
}

#[tokio::test]
async fn exactly_one_thing_is_shown_at_a_time() {
    let rig = Rig::new();
    let site = site(&rig).await;
    let none = show(&site, json!({})).await;
    assert_eq!(none.status(), reqwest::StatusCode::BAD_REQUEST);
    let two = show(
        &site,
        json!({"frame": {"route": "/"}, "focus": {"page": "x"}}),
    )
    .await;
    assert_eq!(two.status(), reqwest::StatusCode::BAD_REQUEST);
    let preset = show(&site, json!({"frame": {"route": "/", "preset": "watch"}})).await;
    assert_eq!(preset.status(), reqwest::StatusCode::BAD_REQUEST);
    let unknown = post(
        &format!("{}/agent/show", site.base),
        json!({"project": "prj_nope", "frame": {"route": "/"}}),
    )
    .await;
    assert_eq!(unknown.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_sketch_near_a_frame_is_drawn_relative_to_it_on_the_agent_layer() {
    let rig = Rig::new();
    let site = site(&rig).await;
    let first = ok(&site, json!({"frame": {"route": "/"}})).await;
    let second = ok(&site, json!({"frame": {"route": "/two"}})).await;
    let page = first["page"].as_str().unwrap().parse().unwrap();
    let second_frame = ItemId::from_hex(second["frame"].as_str().unwrap()).unwrap();
    let origin = {
        let view = rig.view(page).await;
        let item = view.replica.scene().get(second_frame).unwrap();
        (item.transform.e, item.transform.f)
    };
    assert_ne!(
        origin,
        (0.0, 0.0),
        "the second frame has a place of its own"
    );

    let drawn = ok(
        &site,
        json!({"sketch": {
            "near": second["frame"],
            "strokes": [{"points": [[10, 10], [110, 10]], "color": "#00aa00", "width": 4}]
        }}),
    )
    .await;
    assert_eq!(drawn["added"], 1);
    let view = rig.view(page).await;
    let scene = view.replica.scene();
    let agent: Vec<_> = scene
        .live_ids()
        .into_iter()
        .filter_map(|id| scene.get(id))
        .filter(|item| item.layer == Layer::Agent)
        .collect();
    assert_eq!(agent.len(), 1);
    let bounds = scene.bounds(agent[0].id).expect("bounds");
    assert!(
        bounds.min_x >= origin.0
            && bounds.max_x <= origin.0 + 140.0
            && bounds.min_y >= origin.1
            && bounds.max_y <= origin.1 + 40.0,
        "the stroke starts at the frame's corner {origin:?}, got {bounds:?}"
    );
    let focused = rig.focused.lock().unwrap().clone();
    assert_eq!(focused.last().unwrap().1, Some(second_frame));
}

#[tokio::test]
async fn an_image_lands_right_of_the_board_and_a_focus_names_a_frame_or_a_page() {
    let rig = Rig::new();
    let site = site(&rig).await;
    let no_board = show(
        &site,
        json!({"image": {"asset": "0".repeat(64), "w": 10, "h": 10}}),
    )
    .await;
    assert_eq!(no_board.status(), reqwest::StatusCode::NOT_FOUND);

    let opened = ok(&site, json!({"frame": {"route": "/"}})).await;
    let stored = rig.picture(40, 20, [9, 9, 9, 255]);
    let placed = ok(
        &site,
        json!({"image": {"asset": stored.asset, "w": 40, "h": 20, "label": "ref"}}),
    )
    .await;
    assert_eq!(placed["label"], "ref");
    assert!(
        placed["x"].as_f64().unwrap() >= 1440.0,
        "right of the 1440 px frame: {placed}"
    );
    let pinned = ok(
        &site,
        json!({"image": {"asset": stored.asset, "w": 40, "h": 20, "x": 5, "y": 6}}),
    )
    .await;
    assert_eq!(
        (pinned["x"].as_f64(), pinned["y"].as_f64()),
        (Some(5.0), Some(6.0))
    );

    let frame = ok(&site, json!({"focus": {"frame": opened["frame"]}})).await;
    assert_eq!(frame["frame"], opened["frame"]);
    let page = ok(&site, json!({"focus": {"page": opened["page"]}})).await;
    assert_eq!(page["page"], opened["page"]);
    let focused = rig.focused.lock().unwrap().clone();
    assert_eq!(
        focused.last().unwrap().1,
        None,
        "a page focus fits the page"
    );
    let both = show(&site, json!({"focus": {}})).await;
    assert_eq!(both.status(), reqwest::StatusCode::BAD_REQUEST);
}
