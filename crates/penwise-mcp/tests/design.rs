//! Design-harness tools (M3, contract C3) through the real `penwise-mcp` binary: a linked static project, a
//! frame, a mark drawn by a tablet, the bundle with crops, status round trip and the frame snapshot.
//!
//! The bundle resolves the element in headless Chromium (the server's inspect browser), so when Chromium is not
//! installed the test reports that and passes — the feature needs a browser by definition.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_possible_truncation
)]

use std::path::PathBuf;
use std::time::Duration;

use penwise_core::brush::BrushParams;
use penwise_core::edit::Edit;
use penwise_core::item::{Affine, FramePreset, Item, ItemId, ItemKind, Layer, StrokeItem, Z_STEP};
use penwise_core::marks::{Binding, BindingMode, MarkItem, MarkStatus};
use penwise_core::sample::Sample;
use penwise_proto::{ClientKind, ItemRef, PageId};
use penwise_server::serve::{ServerConfig, serve};
use penwise_testkit::{DeviceConfig, IdGen, TempDir, TestDevice, system_clock};
use serde_json::{Value, json};

mod common;

use common::{Mcp, TOKEN};

const PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Portfolio</title>
  <link rel="stylesheet" href="/styles.css">
</head>
<body>
  <main>
    <h1 class="hero">Hi, I make things</h1>
  </main>
</body>
</html>
"#;

const CSS: &str = ".hero { font-size: 32px; font-weight: 800; padding: 120px 20px; margin: 0; min-height: 600px; box-sizing: border-box }\n";

fn chromium_installed() -> bool {
    let path = std::env::var_os("PENWISE_CHROMIUM")
        .map_or_else(|| PathBuf::from("/usr/bin/chromium"), PathBuf::from);
    path.is_file()
}

/// The text form of an item id, whichever form the server chose (hex in the mark API, ULID in the frame API).
fn item_id(text: &str) -> ItemId {
    ItemId::from_hex(text)
        .or_else(|| text.parse::<ItemRef>().ok().map(ItemId::from))
        .unwrap_or_else(|| panic!("item id {text:?} parses"))
}

/// A stroke whose samples are binding-local: a small box around `(0, 0)` of the bound origin.
fn bound_stroke(id: ItemId, z: i64, seed: u64, binding: Binding) -> Item {
    let corners = [[20.0, 20.0], [80.0, 20.0], [80.0, 60.0], [20.0, 60.0]];
    let samples = corners
        .iter()
        .zip(0_u32..)
        .map(|(point, index)| {
            Sample {
                x: point[0],
                y: point[1],
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: index * 8_333,
            }
            .quantize()
        })
        .collect();
    Item {
        id,
        layer: Layer::Ink,
        z,
        transform: Affine::IDENTITY,
        kind: ItemKind::Stroke(StrokeItem {
            params: BrushParams::for_preset(0, 0xFF00_0000, 1.0),
            seed,
            samples,
            trim: None,
            binding: Some(binding),
        }),
    }
}

/// Link the fixture as `prj_site` through the same route `penwise link` uses.
async fn link_site(addr: std::net::SocketAddr, site: &TempDir) {
    let reply = reqwest::Client::new()
        .post(format!("http://{addr}/projects"))
        .json(&json!({
            "id": "prj_site",
            "name": "site",
            "root": site.path(),
            "static": site.path()
        }))
        .send()
        .await
        .expect("link project");
    assert!(
        reply.status().is_success(),
        "link reply: {}",
        reply.text().await.expect("body")
    );
}

fn assert_image(block: &Value) {
    assert_eq!(block["type"], "image", "block: {block}");
    assert_eq!(block["mimeType"], "image/png");
    assert!(
        block["data"]
            .as_str()
            .expect("data")
            .starts_with("iVBORw0KGgo"),
        "a base64 PNG: {block}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mark_bundles_status_and_frames_work_end_to_end() {
    if !chromium_installed() {
        eprintln!("chromium is not installed; MCP design-harness test skipped");
        return;
    }
    let data = TempDir::new("mcp-design-data").expect("data dir");
    let site = TempDir::new("mcp-design-site").expect("site dir");
    std::fs::write(site.path().join("index.html"), PAGE).expect("index.html");
    std::fs::write(site.path().join("styles.css"), CSS).expect("styles.css");
    let mut config = ServerConfig::new(data.path(), TOKEN);
    config.ocr_url = String::new();
    config.project_port_base = 24_000 + (std::process::id() % 1_000) as u16;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let server = serve(listener, config).await.expect("server starts");
    link_site(server.addr, &site).await;

    let mcp = Mcp::start(server.addr.port());
    mcp.rpc("initialize", json!({})).await;

    // Projects and the frame.
    let projects = mcp.json("penwise_projects", json!({})).await;
    assert!(
        projects
            .as_array()
            .expect("projects")
            .iter()
            .any(|project| project["id"] == "prj_site" && project["kind"] == "static"),
        "{projects}"
    );
    let opened = mcp
        .json(
            "penwise_frame_open",
            json!({ "project": "prj_site", "route": "/", "preset": "phone" }),
        )
        .await;
    let page = opened["page"].as_str().expect("page").to_owned();
    let frame = opened["frame"].as_str().expect("frame").to_owned();
    let page_id: PageId = page.parse().expect("page id");

    // A tablet draws one mark on the frame: two area-bound strokes grouped into one mark.
    let mut tablet = TestDevice::new(
        DeviceConfig {
            url: format!("ws://{}/sync", server.addr),
            token: TOKEN.to_owned(),
            kind: ClientKind::Tablet,
            clock: system_clock(),
        },
        9,
    );
    tablet.online().await.expect("tablet online");
    assert!(
        tablet
            .pump_until(Duration::from_secs(10), |device| device
                .replica
                .page(page_id)
                .is_some())
            .await,
        "the tablet sees the page the agent opened"
    );
    let frame_id = item_id(&frame);
    let (fx, fy) = {
        let scene = tablet.replica.page(page_id).expect("page").scene();
        let item = scene.get(frame_id).expect("frame item");
        (item.transform.e, item.transform.f)
    };
    let binding = Binding::new(
        frame_id,
        BindingMode::Area,
        FramePreset::Phone,
        None,
        String::new(),
        [fx, fy],
    )
    .expect("binding");
    let mut ids = IdGen::new(42);
    let first = ids.next_id_item();
    let second = ids.next_id_item();
    let mark = ids.next_id_item();
    let z = {
        let scene = tablet.replica.page(page_id).expect("page").scene();
        scene.next_z().unwrap_or(Z_STEP)
    };
    tablet.push_edits(
        page_id,
        vec![
            Edit::Add(bound_stroke(first, z, 1, binding.clone())),
            Edit::Add(bound_stroke(second, z + 1, 2, binding)),
            Edit::Add(
                MarkItem {
                    frame: frame_id,
                    preset: FramePreset::Phone,
                    status: MarkStatus::New,
                    state: String::new(),
                    strokes: vec![first, second],
                }
                .item(mark, z + 2),
            ),
        ],
    );
    tablet
        .settle(Duration::from_secs(10))
        .await
        .expect("settle");

    // The bundle: JSON text plus the with-ink and context crops.
    let reply = mcp
        .call(
            "penwise_marks",
            json!({ "project": "prj_site", "agent": "design-test", "images": "all" }),
        )
        .await;
    let content = reply["result"]["content"].as_array().expect("content");
    assert_eq!(content[0]["type"], "text");
    let bundle: Value =
        serde_json::from_str(content[0]["text"].as_str().expect("text")).expect("bundle json");
    assert!(bundle["checkpoint"].is_number(), "{bundle}");
    let marks = bundle["marks"].as_array().expect("marks");
    assert_eq!(marks.len(), 1, "{bundle}");
    let mark_json = &marks[0];
    assert_eq!(
        item_id(mark_json["frame"].as_str().expect("mark frame")),
        frame_id
    );
    assert_eq!(mark_json["project"], "prj_site");
    assert_eq!(mark_json["preset"], "phone");
    assert_eq!(mark_json["binding"], "area");
    assert_eq!(mark_json["status"], "new");
    assert!(
        mark_json["kind"]["guess"].is_string(),
        "kind guess: {mark_json}"
    );
    assert!(
        ["problem", "idea", "good", "none"]
            .contains(&mark_json["colour"].as_str().expect("colour")),
        "colour: {mark_json}"
    );
    let element = &mark_json["element"];
    assert!(
        element["selector"]
            .as_str()
            .expect("selector")
            .contains("hero"),
        "the mark resolves to the hero: {element}"
    );
    let sources = element["source"].as_array().expect("source");
    assert!(
        sources
            .iter()
            .any(|source| source["file"] == "index.html" && source["line"].is_number()),
        "index.html line: {element}"
    );
    assert!(
        sources.iter().any(|source| source["file"] == "styles.css"),
        "styles.css rule: {element}"
    );
    let images = &mark_json["images"];
    for kind in ["with_ink", "without_ink", "context"] {
        assert!(
            images[kind].as_str().expect(kind).starts_with("/marks/"),
            "{kind}: {images}"
        );
    }
    assert_eq!(content.len(), 3, "one text block and two crops");
    assert_image(&content[1]);
    assert_image(&content[2]);

    // The same agent's next check is empty: the checkpoint hides marks it already saw.
    let again = mcp
        .json(
            "penwise_marks",
            json!({ "project": "prj_site", "agent": "design-test", "images": "none" }),
        )
        .await;
    assert_eq!(again["marks"], json!([]), "{again}");
    assert!(
        again["checkpoint"].as_u64().expect("checkpoint")
            >= bundle["checkpoint"].as_u64().expect("checkpoint"),
        "the checkpoint never moves backwards"
    );

    // Status round trip: plain images off, and a second agent sees the new status.
    let seen = mcp
        .json(
            "penwise_mark_status",
            json!({ "id": mark_json["id"], "status": "seen", "note": "read it" }),
        )
        .await;
    assert_eq!(seen["status"], "seen");
    let listed = mcp
        .json(
            "penwise_marks",
            json!({
                "project": "prj_site",
                "status": "seen",
                "since": "none",
                "images": "none",
                "agent": "design-test-reader"
            }),
        )
        .await;
    assert_eq!(listed["marks"][0]["status"], "seen", "{listed}");
    assert_eq!(listed["marks"][0]["id"], mark_json["id"]);

    // Snapshot of the frame as it stands (with ink).
    let shot = mcp
        .call(
            "penwise_frame_snapshot",
            json!({ "frame": frame, "ink": true }),
        )
        .await;
    let shot = shot["result"]["content"]
        .as_array()
        .expect("snapshot content");
    assert_eq!(shot[0]["type"], "text");
    assert_image(&shot[1]);

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ghost_variants_and_choice_work_end_to_end() {
    if !chromium_installed() {
        eprintln!("chromium is not installed; MCP ghost/variants test skipped");
        return;
    }
    let data = TempDir::new("mcp-variants-data").expect("data dir");
    let site = TempDir::new("mcp-variants-site").expect("site dir");
    std::fs::write(site.path().join("index.html"), PAGE).expect("index.html");
    std::fs::write(site.path().join("styles.css"), CSS).expect("styles.css");
    let mut config = ServerConfig::new(data.path(), TOKEN);
    config.ocr_url = String::new();
    config.project_port_base = 25_000 + (std::process::id() % 1_000) as u16;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let server = serve(listener, config).await.expect("server starts");
    link_site(server.addr, &site).await;

    let mcp = Mcp::start(server.addr.port());
    mcp.rpc("initialize", json!({})).await;
    let opened = mcp
        .json(
            "penwise_frame_open",
            json!({ "project": "prj_site", "route": "/", "preset": "phone" }),
        )
        .await;
    let page = opened["page"].as_str().expect("page").to_owned();
    let frame = opened["frame"].as_str().expect("frame").to_owned();
    let page_id: PageId = page.parse().expect("page id");

    // A ghost: applied, with no failures; a stale selector comes back as a failure.
    let ghost = mcp
        .json(
            "penwise_ghost",
            json!({ "frame": frame, "patch": {
                "css": ".hero { color: rgb(1, 2, 3) }",
                "dom": [{ "op": "set_text", "selector": ".hero", "value": "Ghost" }]
            } }),
        )
        .await;
    assert_eq!(ghost["ok"], json!(true), "{ghost}");
    assert_eq!(ghost["failed"], json!([]));
    let failed = mcp
        .json(
            "penwise_ghost",
            json!({ "frame": frame, "patch": { "dom": [{ "op": "remove", "selector": ".nope" }] } }),
        )
        .await;
    assert_eq!(failed["ok"], json!(false), "{failed}");
    assert_eq!(failed["failed"][0]["reason"], "no match");
    let cleared = mcp
        .json("penwise_ghost", json!({ "frame": frame, "clear": true }))
        .await;
    assert_eq!(cleared["ok"], json!(true));

    // Three variants beside the base frame.
    let created = mcp
        .json(
            "penwise_variants",
            json!({ "frame": frame, "variants": [
                { "label": "calm", "patch": { "css": ".hero { color: gray }" } },
                { "label": "bold", "patch": { "css": ".hero { font-weight: 900 }" } },
                { "label": "short", "patch": { "dom": [{ "op": "set_text", "selector": ".hero", "value": "Hi" }] } }
            ] }),
        )
        .await;
    let group = created["group"].as_str().expect("group").to_owned();
    let frames: Vec<String> = created["frames"]
        .as_array()
        .expect("frames")
        .iter()
        .map(|id| id.as_str().expect("id").to_owned())
        .collect();
    assert_eq!(frames.len(), 3, "{created}");

    // Without a pick, the choice is null and the three labels are there.
    let before = mcp.json("penwise_choice", json!({ "group": group })).await;
    assert_eq!(before["chosen"], Value::Null);
    assert_eq!(
        before["frames"].as_array().expect("frames")[1]["label"],
        "bold"
    );

    // A tablet sees the variants, taps one ("Use this" = a FrameChosen edit) and draws on it.
    let mut tablet = TestDevice::new(
        DeviceConfig {
            url: format!("ws://{}/sync", server.addr),
            token: TOKEN.to_owned(),
            kind: ClientKind::Tablet,
            clock: system_clock(),
        },
        10,
    );
    tablet.online().await.expect("tablet online");
    let variant_ids: Vec<ItemId> = frames.iter().map(|text| item_id(text)).collect();
    let seen = tablet
        .pump_until(Duration::from_secs(10), |device| {
            device.replica.page(page_id).is_some_and(|page| {
                let scene = page.scene();
                variant_ids.iter().all(|id| {
                    scene
                        .get(*id)
                        .is_some_and(|item| matches!(item.kind, ItemKind::Frame(_)))
                })
            })
        })
        .await;
    assert!(seen, "the tablet sees the variant frames");

    let chosen_id = item_id(&frames[1]);
    let edits = {
        let scene = tablet.replica.page(page_id).expect("page").scene();
        penwise_core::item::set_chosen(scene, chosen_id, true).expect("set_chosen")
    };
    let mut ids = IdGen::new(77);
    let stroke = ids.next_id_item();
    let mark = ids.next_id_item();
    let (fx, fy, z) = {
        let scene = tablet.replica.page(page_id).expect("page").scene();
        let item = scene.get(chosen_id).expect("chosen variant");
        (
            item.transform.e,
            item.transform.f,
            scene.next_z().unwrap_or(Z_STEP),
        )
    };
    let binding = Binding::new(
        chosen_id,
        BindingMode::Area,
        FramePreset::Phone,
        None,
        String::new(),
        [fx, fy],
    )
    .expect("binding");
    let mut batch = edits;
    batch.push(Edit::Add(bound_stroke(stroke, z, 3, binding)));
    batch.push(Edit::Add(
        MarkItem {
            frame: chosen_id,
            preset: FramePreset::Phone,
            status: MarkStatus::New,
            state: String::new(),
            strokes: vec![stroke],
        }
        .item(mark, z + 1),
    ));
    tablet.push_edits(page_id, batch);
    tablet
        .settle(Duration::from_secs(10))
        .await
        .expect("settle");

    // The pick and the ink reach the agent: the text block names the chosen frame, an image block follows.
    let reply = mcp.call("penwise_choice", json!({ "group": group })).await;
    let content = reply["result"]["content"].as_array().expect("content");
    let choice: Value =
        serde_json::from_str(content[0]["text"].as_str().expect("text")).expect("choice json");
    assert_eq!(choice["chosen"], frames[1], "{choice}");
    assert_eq!(choice["frames"][1]["chosen"], json!(true));
    assert_eq!(choice["frames"][0]["chosen"], json!(false));
    let marks = choice["marks"].as_array().expect("marks");
    assert_eq!(marks.len(), 1, "{choice}");
    assert_eq!(marks[0]["frame"], frames[1]);
    assert!(content.len() > 1, "the crops are inline");
    assert_image(&content[1]);

    // Clearing removes the frames; the group is gone.
    let cleared = mcp
        .json("penwise_variants_clear", json!({ "group": group }))
        .await;
    assert_eq!(cleared["removed"], json!(3));
    assert_eq!(cleared["marks"], json!(1));
    let gone = mcp
        .rpc(
            "tools/call",
            json!({ "name": "penwise_choice", "arguments": { "group": group } }),
        )
        .await;
    assert!(gone["error"].is_object(), "{gone}");

    server.shutdown().await;
}
