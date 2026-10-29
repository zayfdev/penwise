//! Drives every tool of `docs/mcp.md` through the real `penwise-mcp` binary (stdio JSON-RPC) against an
//! in-process server on 127.0.0.1:0 with a temp data dir and a fake handwriting model.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use std::time::Duration;

use penwise_proto::ClientKind;
use penwise_server::serve::{ServerConfig, serve};
use penwise_testkit::{DeviceConfig, TempDir, TestDevice, system_clock};
use serde_json::{Value, json};

mod common;

use common::{Mcp, TOKEN};

/// The tools `docs/mcp.md` documents.
const DOCUMENTED: [&str; 19] = [
    "penwise_page_png",
    "penwise_page_status",
    "penwise_annotate",
    "penwise_clear_agent_marks",
    "penwise_list_pages",
    "penwise_create_page",
    "penwise_create_notebook",
    "penwise_page_text",
    "penwise_search",
    "penwise_switch_page",
    "penwise_place_image",
    "penwise_list_images",
    "penwise_remove_image",
    "penwise_page_annotations",
    "penwise_projects",
    "penwise_frame_open",
    "penwise_frame_snapshot",
    "penwise_marks",
    "penwise_mark_status",
];

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

fn stroke_count(status: &Value) -> u64 {
    status["strokes"].as_u64().expect("strokes")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_documented_tool_works_against_the_server() {
    let data = TempDir::new("mcp-server").expect("data dir");
    let scratch = TempDir::new("mcp-files").expect("scratch");
    let mut config = ServerConfig::new(data.path(), TOKEN);
    config.ocr_url = fake_model().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let server = serve(listener, config).await.expect("server starts");
    let mcp = Mcp::start(server.addr.port());

    let init = mcp.rpc("initialize", json!({})).await;
    assert_eq!(init["result"]["serverInfo"]["name"], "penwise");
    let listed = mcp.rpc("tools/list", json!({})).await;
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    for tool in DOCUMENTED {
        assert!(names.contains(&tool), "{tool} is listed");
    }
    assert_eq!(
        names.len(),
        DOCUMENTED.len(),
        "no undocumented tool: {names:?}"
    );

    // Pages and notebooks.
    let empty = mcp.json("penwise_list_pages", json!({})).await;
    assert_eq!(empty["notebooks"], json!([]));
    let one = mcp
        .json("penwise_create_page", json!({ "title": "One" }))
        .await;
    let p1 = one["page"]["id"].as_str().expect("page id").to_owned();
    assert_eq!(one["page"]["title"], "One");
    let book = mcp
        .json("penwise_create_notebook", json!({ "title": "Work" }))
        .await;
    let nb = book["notebook"]["id"].as_str().expect("nb id").to_owned();
    let two = mcp
        .json(
            "penwise_create_page",
            json!({ "title": "Two", "notebook": nb, "background": "grid", "kind": { "t": "infinite" } }),
        )
        .await;
    let p2 = two["page"]["id"].as_str().expect("page id").to_owned();
    assert_eq!(two["page"]["background"], "grid");
    assert_eq!(two["page"]["kind"]["t"], "infinite");
    let listing = mcp.json("penwise_list_pages", json!({})).await;
    assert_eq!(listing["notebooks"].as_array().expect("nbs").len(), 2);
    assert_eq!(
        listing["active"], p1,
        "the first page is current until one is chosen"
    );
    let switched = mcp.json("penwise_switch_page", json!({ "page": p2 })).await;
    assert_eq!(switched["active"], p2);
    assert_eq!(
        mcp.json("penwise_list_pages", json!({})).await["active"],
        p2
    );

    // Annotate / status / clear on the active page.
    let status = mcp.json("penwise_page_status", json!({})).await;
    assert_eq!(status["ok"], true);
    assert_eq!(stroke_count(&status), 0);
    let added = mcp
        .json(
            "penwise_annotate",
            json!({ "strokes": [{ "points": [[10, 10], [200, 120]], "color": "#00aa00", "width": 5 }] }),
        )
        .await;
    assert_eq!(added["added"], 1);
    assert_eq!(
        stroke_count(&mcp.json("penwise_page_status", json!({})).await),
        1
    );
    let on_p1 = mcp.json("penwise_page_status", json!({ "page": p1 })).await;
    assert_eq!(stroke_count(&on_p1), 0, "annotate hit the active page only");

    // PNG.
    let png = mcp
        .call("penwise_page_png", json!({ "scale": 0.5, "page": p1 }))
        .await;
    let content = png["result"]["content"].as_array().expect("content");
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/png");
    assert!(
        content[1]["data"]
            .as_str()
            .expect("data")
            .starts_with("iVBORw0KGgo"),
        "base64 PNG magic"
    );

    // Pictures and anchors.
    let file = png_file(&scratch);
    let placed = mcp
        .json(
            "penwise_place_image",
            json!({ "path": file, "x": 20, "y": 30, "label": "ref" }),
        )
        .await;
    let image_id = placed["id"].as_str().expect("picture id").to_owned();
    assert_eq!(placed["label"], "ref");
    let images = mcp.json("penwise_list_images", json!({})).await;
    assert_eq!(images["page"], p2);
    assert_eq!(images["images"].as_array().expect("images").len(), 1);
    let anchors = mcp.json("penwise_page_annotations", json!({})).await;
    assert!(anchors["unanchored"].is_number(), "{anchors}");
    let removed = mcp
        .json("penwise_remove_image", json!({ "id": image_id }))
        .await;
    assert_eq!(removed["removed"], 1);
    let images = mcp.json("penwise_list_images", json!({})).await;
    assert_eq!(images["images"], json!([]));

    // Text and search need ink: a tablet draws on p2 over /sync.
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
    let page: penwise_proto::PageId = p2.parse().expect("page id parses");
    assert!(
        tablet
            .pump_until(Duration::from_secs(10), |d| d.replica.page(page).is_some())
            .await,
        "the tablet sees the page the agent created"
    );
    let points: Vec<[f32; 2]> = (0..40).map(|i| [20.0 + i as f32 * 5.0, 300.0]).collect();
    tablet.draw(page, &points);
    tablet
        .settle(Duration::from_secs(10))
        .await
        .expect("settle");
    let text = mcp.json("penwise_page_text", json!({})).await;
    assert_eq!(text["text"], "hello penwise");
    assert_eq!(text["cached"], false);
    let again = mcp.json("penwise_page_text", json!({})).await;
    assert_eq!(again["cached"], true);
    let refreshed = mcp
        .json("penwise_page_text", json!({ "refresh": true }))
        .await;
    assert_eq!(refreshed["cached"], false);
    let hits = mcp
        .json("penwise_search", json!({ "query": "PENWISE" }))
        .await;
    assert_eq!(hits["hits"].as_array().expect("hits").len(), 1, "{hits}");
    assert_eq!(hits["hits"][0]["page"], p2);

    // Clearing agent marks leaves the tablet's ink.
    let cleared = mcp.json("penwise_clear_agent_marks", json!({})).await;
    assert_eq!(cleared["removed"], 1);
    assert_eq!(
        stroke_count(&mcp.json("penwise_page_status", json!({})).await),
        1,
        "only the ink stroke is left"
    );
    server.shutdown().await;
}
