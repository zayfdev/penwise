//! HTTP tests for the projects API and frame creation, against the real router on a loopback socket.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use penwise_core::item::{FramePreset, ItemKind};
use serde_json::{Value, json};

use super::routes;
use crate::testing::Rig;

const INDEX: &str = "<!doctype html><html><head></head><body><h1>portfolio</h1></body></html>";

fn site(dir: &Path) -> PathBuf {
    let root = dir.join("portfolio");
    std::fs::create_dir_all(&root).expect("site");
    std::fs::write(root.join("index.html"), INDEX).expect("index");
    root
}

async fn post(url: &str, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(url)
        .json(&body)
        .send()
        .await
        .expect("post")
}

async fn create_project(base: &str, root: &Path) -> Value {
    let response = post(
        &format!("{base}/projects"),
        json!({"name": "portfolio", "root": root, "static": root}),
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    response.json().await.expect("project json")
}

#[tokio::test]
async fn projects_allow_loopback_without_a_token_but_not_another_host() {
    let rig = Rig::new();
    let base = rig.serve(routes(Arc::new("secret".to_owned()))).await;
    let client = reqwest::Client::new();
    let local = client
        .get(format!("{base}/projects"))
        .send()
        .await
        .expect("local");
    assert_eq!(local.status(), reqwest::StatusCode::OK);

    let foreign = client
        .get(format!("{base}/projects"))
        .header(reqwest::header::HOST, "example.com")
        .send()
        .await
        .expect("foreign host");
    assert_eq!(foreign.status(), reqwest::StatusCode::UNAUTHORIZED);

    let header_token = client
        .get(format!("{base}/projects"))
        .header(reqwest::header::HOST, "example.com")
        .header("x-penwise-token", "secret")
        .send()
        .await
        .expect("header token");
    assert_eq!(header_token.status(), reqwest::StatusCode::OK);

    let query_token = client
        .get(format!("{base}/projects?token=secret"))
        .header(reqwest::header::HOST, "example.com")
        .send()
        .await
        .expect("query token");
    assert_eq!(query_token.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn the_loopback_bypass_refuses_requests_that_a_browser_marks_as_another_origin() {
    let rig = Rig::new();
    let base = rig.serve(routes(Arc::new("secret".to_owned()))).await;
    let own_origin = base.clone();
    let client = reqwest::Client::new();
    let get = |headers: &[(&str, &str)]| {
        let mut request = client.get(format!("{base}/projects"));
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        async move { request.send().await.expect("request").status() }
    };
    let ok = reqwest::StatusCode::OK;
    let denied = reqwest::StatusCode::UNAUTHORIZED;

    assert_eq!(get(&[]).await, ok, "the CLI sends no Origin");
    assert_eq!(
        get(&[("origin", &own_origin)]).await,
        ok,
        "the PC viewer is the server's own origin"
    );
    assert_eq!(get(&[("sec-fetch-site", "same-origin")]).await, ok);
    assert_eq!(get(&[("sec-fetch-site", "none")]).await, ok);
    assert_eq!(
        get(&[("origin", "http://127.0.0.1:7421")]).await,
        denied,
        "a dev site on a project port is another origin"
    );
    assert_eq!(get(&[("origin", "https://evil.example")]).await, denied);
    assert_eq!(get(&[("origin", "null")]).await, denied);
    assert_eq!(get(&[("sec-fetch-site", "cross-site")]).await, denied);
    assert_eq!(get(&[("sec-fetch-site", "same-site")]).await, denied);
    assert_eq!(
        get(&[
            ("origin", "https://evil.example"),
            ("x-penwise-token", "secret")
        ])
        .await,
        ok,
        "the token still opens the door from anywhere"
    );
}

#[tokio::test]
async fn look_has_effects_so_it_is_a_post_and_a_get_is_refused() {
    let rig = Rig::new();
    let base = rig.serve(routes(Arc::new(String::new()))).await;
    let client = reqwest::Client::new();
    let get = client
        .get(format!("{base}/agent/look?project=prj_x"))
        .send()
        .await
        .expect("get look");
    assert_eq!(get.status(), reqwest::StatusCode::METHOD_NOT_ALLOWED);
    let post = client
        .post(format!("{base}/agent/look"))
        .json(&json!({ "project": "prj_x" }))
        .send()
        .await
        .expect("post look");
    assert_eq!(
        post.status(),
        reqwest::StatusCode::NOT_FOUND,
        "unknown project"
    );
}

#[tokio::test]
async fn linking_registers_the_project_creates_its_notebook_and_serves_it() {
    let rig = Rig::new();
    let root = site(rig.dir.path());
    let base = rig.serve(routes(Arc::new(String::new()))).await;
    let project = create_project(&base, &root).await;

    assert!(project["id"].as_str().unwrap().starts_with("prj_"));
    assert_eq!(project["name"], "portfolio");
    assert_eq!(project["root"], root.to_string_lossy().as_ref());
    assert_eq!(project["kind"], "static");
    assert_eq!(project["static"], root.to_string_lossy().as_ref());
    assert_eq!(project["origin"], Value::Null);
    let port = project["port"].as_u64().unwrap();
    assert!(port >= 17_421, "tests never use the production range");
    let notebook = project["notebook"].as_str().expect("notebook");
    assert!(
        rig.storage
            .catalog()
            .await
            .unwrap()
            .notebooks()
            .iter()
            .any(|nb| nb.id.to_string() == notebook && nb.title == "portfolio"),
        "the notebook is named after the project"
    );

    let html = reqwest::get(format!("http://127.0.0.1:{port}/"))
        .await
        .expect("project port")
        .text()
        .await
        .expect("html");
    assert!(html.contains("portfolio"));
    assert!(html.contains("/__penwise/bridge.js"));

    let listed: Value = reqwest::Client::new()
        .get(format!("{base}/projects"))
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("list json");
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["id"], project["id"]);
}

#[tokio::test]
async fn relinking_is_idempotent_and_keeps_the_notebook() {
    let rig = Rig::new();
    let root = site(rig.dir.path());
    let base = rig.serve(routes(Arc::new(String::new()))).await;
    let first = create_project(&base, &root).await;
    let second = create_project(&base, &root).await;
    assert_eq!(second["id"], first["id"]);
    assert_eq!(second["port"], first["port"]);
    assert_eq!(second["notebook"], first["notebook"]);
    assert_eq!(
        rig.storage.catalog().await.unwrap().notebooks().len(),
        1,
        "no second notebook"
    );
}

#[tokio::test]
async fn delete_unregisters_and_stops_the_port() {
    let rig = Rig::new();
    let root = site(rig.dir.path());
    let base = rig.serve(routes(Arc::new(String::new()))).await;
    let project = create_project(&base, &root).await;
    let id = project["id"].as_str().unwrap().to_owned();
    let port = u16::try_from(project["port"].as_u64().unwrap()).unwrap();
    let response = reqwest::Client::new()
        .delete(format!("{base}/projects/{id}"))
        .send()
        .await
        .expect("delete");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let listed: Value = reqwest::Client::new()
        .get(format!("{base}/projects"))
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("list json");
    assert!(listed.as_array().unwrap().is_empty());
    assert!(
        reqwest::get(format!("http://127.0.0.1:{port}/"))
            .await
            .is_err(),
        "the removed project's port is closed"
    );
    let missing = reqwest::Client::new()
        .delete(format!("{base}/projects/{id}"))
        .send()
        .await
        .expect("delete");
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn frame_create_adds_a_frame_item_with_the_preset_size() {
    let mut rig = Rig::new();
    let root = site(rig.dir.path());
    let page = rig.fixed_page().await;
    let base = rig.serve(routes(Arc::new(String::new()))).await;
    let project = create_project(&base, &root).await;
    let project_id = project["id"].as_str().unwrap().to_owned();

    let response = post(
        &format!("{base}/pages/{page}/frames"),
        json!({
            "project": project_id,
            "route": "/",
            "preset": "phone",
            "x": 120.5,
            "y": -40.0,
        }),
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let created: Value = response.json().await.expect("frame json");
    let id = created["id"].as_str().expect("frame id").to_owned();

    let view = rig.view(page).await;
    let (item, frame) = view
        .replica
        .items()
        .filter(|state| !state.deleted)
        .find_map(|state| match &state.item.kind {
            ItemKind::Frame(frame) => Some((&state.item, frame)),
            _ => None,
        })
        .expect("a frame item");
    assert_eq!(penwise_proto::ItemRef::from(item.id).to_string(), id);
    assert_eq!(frame.project, project_id);
    assert_eq!(frame.route, "/");
    assert_eq!(frame.preset, FramePreset::Phone);
    assert_eq!((frame.css_w, frame.css_h), (390, 844));
    let size = frame.page_size();
    assert!((size[0] - 390.0).abs() < f32::EPSILON);
    assert!((size[1] - 844.0).abs() < f32::EPSILON);
    assert!((item.transform.e - 120.5).abs() < f32::EPSILON);
    assert!((item.transform.f + 40.0).abs() < f32::EPSILON);
}

#[tokio::test]
async fn frame_create_accepts_a_custom_size_and_rejects_bad_input() {
    let mut rig = Rig::new();
    let root = site(rig.dir.path());
    let page = rig.fixed_page().await;
    let base = rig.serve(routes(Arc::new(String::new()))).await;
    let project = create_project(&base, &root).await;
    let project_id = project["id"].as_str().unwrap().to_owned();
    let frames = format!("{base}/pages/{page}/frames");

    let response = post(
        &frames,
        json!({
            "project": project_id,
            "route": "/work?tag=one",
            "preset": "custom",
            "cssW": 700,
            "cssH": 500,
        }),
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let unknown_preset = post(
        &frames,
        json!({"project": project_id, "route": "/", "preset": "watch"}),
    )
    .await;
    assert_eq!(unknown_preset.status(), reqwest::StatusCode::BAD_REQUEST);

    let no_size = post(
        &frames,
        json!({"project": project_id, "route": "/", "preset": "custom"}),
    )
    .await;
    assert_eq!(no_size.status(), reqwest::StatusCode::BAD_REQUEST);

    let bad_route = post(
        &frames,
        json!({"project": project_id, "route": "work", "preset": "phone"}),
    )
    .await;
    assert_eq!(bad_route.status(), reqwest::StatusCode::BAD_REQUEST);

    let unknown_project = post(
        &frames,
        json!({"project": "prj_missing", "route": "/", "preset": "phone"}),
    )
    .await;
    assert_eq!(unknown_project.status(), reqwest::StatusCode::NOT_FOUND);
}
