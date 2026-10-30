//! The four tools of `docs/mcp.md` through the real `penwise-mcp` binary (stdio JSON-RPC) against an in-process
//! server on 127.0.0.1:0 with a temp data dir and a fake handwriting model. Nothing here needs a browser: the
//! round with real marks and pixels is `tests/round.rs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use std::time::Duration;

use penwise_proto::{ClientKind, PageId};
use penwise_server::serve::{ServerConfig, serve};
use penwise_testkit::{DeviceConfig, TempDir, TestDevice, system_clock};
use serde_json::{Value, json};

mod common;

use common::{Mcp, TOKEN};

/// A fake Ollama that "reads" every page as the same words.
async fn fake_model() -> String {
    let app = axum::Router::new().route(
        "/api/generate",
        axum::routing::post(|| async { axum::Json(json!({ "response": "hello penwise" })) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind model");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    url
}

fn png_file(dir: &TempDir) -> String {
    let mut pixmap = tiny_skia::Pixmap::new(24, 16).expect("pixmap");
    pixmap.fill(tiny_skia::Color::from_rgba8(30, 120, 200, 255));
    let path = dir.path().join("ref.png");
    std::fs::write(&path, pixmap.encode_png().expect("png")).expect("write png");
    path.to_string_lossy().into_owned()
}

async fn link_site(addr: std::net::SocketAddr, site: &TempDir) {
    std::fs::write(site.path().join("index.html"), "<!doctype html><h1>hi</h1>").expect("index");
    let reply = reqwest::Client::new()
        .post(format!("http://{addr}/projects"))
        .json(&json!({ "id": "prj_site", "name": "site", "root": site.path(), "static": site.path() }))
        .send()
        .await
        .expect("link project");
    assert!(
        reply.status().is_success(),
        "{}",
        reply.text().await.unwrap()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exactly_four_tools_are_listed_with_their_schemas() {
    let data = TempDir::new("mcp-list").expect("data dir");
    let mut config = ServerConfig::new(data.path(), TOKEN);
    config.ocr_url = String::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let server = serve(listener, config).await.expect("server starts");
    let mcp = Mcp::start(server.addr.port());

    let init = mcp.rpc("initialize", json!({})).await;
    assert_eq!(init["result"]["serverInfo"]["name"], "penwise");
    let listed = mcp.rpc("tools/list", json!({})).await;
    let tools = listed["result"]["tools"].as_array().expect("tools");
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["look", "show", "resolve", "notes"]);
    let props = |tool: &Value| -> Vec<String> {
        tool["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    };
    let mut look = props(&tools[0]);
    look.sort();
    assert_eq!(look, ["frame", "images", "mark"]);
    let mut show = props(&tools[1]);
    show.sort();
    assert_eq!(
        show,
        ["focus", "frame", "ghost", "image", "sketch", "variants"]
    );
    assert_eq!(
        tools[1]["inputSchema"]["maxProperties"], 1,
        "exactly one thing"
    );
    let mut resolve = props(&tools[2]);
    resolve.sort();
    assert_eq!(resolve, ["marks", "round"]);
    let mut notes = props(&tools[3]);
    notes.sort();
    assert_eq!(notes, ["page", "png", "query"]);
    for tool in tools {
        assert!(
            tool["inputSchema"]["properties"].get("project").is_none(),
            "no tool takes a project argument: {}",
            tool["name"]
        );
    }
    let size = serde_json::to_string(&listed["result"]).unwrap().len();
    assert!(size < 6_000, "the schemas stay small: {size} bytes");

    let old = mcp.call_raw("penwise_page_png", json!({})).await;
    assert!(old["error"].is_object(), "v1 tools are gone: {old}");
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_board_tools_need_the_linked_project_and_work_on_an_empty_board() {
    let data = TempDir::new("mcp-board").expect("data dir");
    let site = TempDir::new("mcp-board-site").expect("site");
    let scratch = TempDir::new("mcp-board-files").expect("scratch");
    let mut config = ServerConfig::new(data.path(), TOKEN);
    config.ocr_url = String::new();
    config.project_port_base = 26_000 + (std::process::id() % 1_000) as u16;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let server = serve(listener, config).await.expect("server starts");
    link_site(server.addr, &site).await;
    let port = server.addr.port();

    // Without PENWISE_PROJECT the board tools refuse and say why; notes does not need it.
    let loose = Mcp::start(port);
    let refused = loose.failure("look", json!({})).await;
    assert!(refused.contains("PENWISE_PROJECT"), "{refused}");

    let mcp = Mcp::start_for(port, "prj_site");
    // An empty board: text only, a hint to open a frame.
    let reply = mcp.call("look", json!({})).await;
    let content = reply["result"]["content"].as_array().unwrap();
    assert_eq!(content.len(), 1, "no marks, no images");
    let text = content[0]["text"].as_str().unwrap();
    assert!(text.contains("round 1 open"), "{text}");
    assert!(text.contains("board none yet"), "{text}");
    // `round: true` never closes an open round; naming the open round with no marks is a conflict.
    let empty = mcp.failure("resolve", json!({ "round": true })).await;
    assert!(empty.contains("no sent round"), "{empty}");
    let open_empty = mcp.failure("resolve", json!({ "round": 1 })).await;
    assert!(
        open_empty.contains("no marks"),
        "the server's reason reaches the agent: {open_empty}"
    );
    let unknown_round = mcp.failure("resolve", json!({ "round": 9 })).await;
    assert!(unknown_round.contains("unknown round"), "{unknown_round}");
    let neither = mcp.failure("resolve", json!({})).await;
    assert!(neither.contains("marks"), "{neither}");

    // show frame: desktop, full length, and the same call again reuses the frame.
    let opened = mcp.json("show", json!({ "frame": { "route": "/" } })).await;
    assert_eq!(opened["preset"], "desktop");
    assert_eq!(opened["extent"], "full");
    assert_eq!(opened["reused"], false);
    let again = mcp.json("show", json!({ "frame": { "route": "/" } })).await;
    assert_eq!(again["reused"], true);
    assert_eq!(again["frame"], opened["frame"]);
    let two = mcp
        .failure(
            "show",
            json!({ "frame": { "route": "/" }, "focus": { "page": "x" } }),
        )
        .await;
    assert!(two.contains("exactly one"), "two things at once: {two}");

    // sketch near the frame, image from a local file, focus.
    let sketch = mcp
        .json(
            "show",
            json!({ "sketch": { "near": opened["frame"], "strokes": [{ "points": [[10, 10], [200, 120]], "color": "#00aa00", "width": 5 }] } }),
        )
        .await;
    assert_eq!(sketch["added"], 1);
    let file = png_file(&scratch);
    let image = mcp
        .json("show", json!({ "image": { "path": file, "label": "ref" } }))
        .await;
    assert_eq!(image["label"], "ref");
    // Only regular files up to the size cap are read.
    let dir_path = scratch.path().to_string_lossy().into_owned();
    let not_a_file = mcp
        .failure("show", json!({ "image": { "path": dir_path } }))
        .await;
    assert!(not_a_file.contains("not a regular file"), "{not_a_file}");
    let big = scratch.path().join("big.png");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(21 * 1024 * 1024)
        .unwrap();
    let too_big = mcp
        .failure(
            "show",
            json!({ "image": { "path": big.to_string_lossy() } }),
        )
        .await;
    assert!(too_big.contains("limit"), "{too_big}");
    assert_eq!(
        (image["w"].as_f64(), image["h"].as_f64()),
        (Some(24.0), Some(16.0))
    );
    let focus = mcp
        .json("show", json!({ "focus": { "frame": opened["frame"] } }))
        .await;
    assert_eq!(focus["frame"], opened["frame"]);

    // The board page now shows the frame, the sketch and the picture.
    let png = mcp
        .call("notes", json!({ "png": true, "page": opened["page"] }))
        .await;
    let blocks = png["result"]["content"].as_array().unwrap();
    assert_eq!(blocks[1]["type"], "image");
    assert!(
        blocks[1]["data"]
            .as_str()
            .unwrap()
            .starts_with("iVBORw0KGgo"),
        "a base64 PNG"
    );

    // The owner sends nothing yet: the open round has no marks, so Send is a conflict.
    let send = reqwest::Client::new()
        .post(format!(
            "http://{}/projects/prj_site/rounds/send",
            server.addr
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(send.status(), reqwest::StatusCode::CONFLICT);

    // A server refusal reaches the agent with its reason, as a tool error it can read.
    let missing = mcp
        .failure("look", json!({ "mark": "00".repeat(16) }))
        .await;
    assert!(missing.contains("404"), "{missing}");
    assert!(
        missing.contains("unknown mark"),
        "the response body is kept: {missing}"
    );
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn notes_lists_pages_reads_handwriting_and_searches() {
    let data = TempDir::new("mcp-notes").expect("data dir");
    let mut config = ServerConfig::new(data.path(), TOKEN);
    config.ocr_url = fake_model().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let server = serve(listener, config).await.expect("server starts");
    let mcp = Mcp::start(server.addr.port());
    mcp.rpc("initialize", json!({})).await;

    // Pages are made through the server's page route (the agent has no tool for it any more).
    let created: Value = reqwest::Client::new()
        .post(format!("http://{}/pages", server.addr))
        .header("X-Penwise-Token", TOKEN)
        .json(&json!({ "title": "One" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let page = created["page"]["id"].as_str().unwrap().to_owned();

    let listing: Value = serde_json::from_str(&mcp.text("notes", json!({})).await).unwrap();
    assert_eq!(
        listing["notebooks"].as_array().unwrap().len(),
        1,
        "{listing}"
    );
    assert_eq!(listing["active"], page);

    let mut tablet = TestDevice::new(
        DeviceConfig {
            url: format!("ws://{}/sync", server.addr),
            token: TOKEN.to_owned(),
            kind: ClientKind::Tablet,
            clock: system_clock(),
        },
        1,
    );
    tablet.online().await.expect("tablet online");
    let page_id: PageId = page.parse().expect("page id parses");
    assert!(
        tablet
            .pump_until(Duration::from_secs(10), |d| d
                .replica
                .page(page_id)
                .is_some())
            .await,
        "the tablet sees the page"
    );
    let points: Vec<[f32; 2]> = (0..40).map(|i| [20.0 + i as f32 * 5.0, 300.0]).collect();
    tablet.draw(page_id, &points);
    tablet
        .settle(Duration::from_secs(10))
        .await
        .expect("settle");

    let text: Value =
        serde_json::from_str(&mcp.text("notes", json!({ "page": page })).await).unwrap();
    assert_eq!(text["text"], "hello penwise");
    let hits: Value =
        serde_json::from_str(&mcp.text("notes", json!({ "query": "PENWISE" })).await).unwrap();
    assert_eq!(hits["hits"].as_array().unwrap().len(), 1, "{hits}");
    assert_eq!(hits["hits"][0]["page"], page);

    let png = mcp
        .call("notes", json!({ "png": true, "page": page }))
        .await;
    let blocks = png["result"]["content"].as_array().unwrap();
    assert_eq!(blocks[1]["mimeType"], "image/png");
    server.shutdown().await;
}
