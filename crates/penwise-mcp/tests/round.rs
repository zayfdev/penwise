//! A whole design round through the real `penwise-mcp` binary, the way the owner and an agent live it: the agent
//! shows a frame, the owner marks it on a tablet and sends, the agent looks, proposes variants, the owner picks
//! one, the agent resolves. The agent's part after the owner's first action is four tool calls.
//!
//! The server resolves elements and renders pixels in headless Chromium, so when Chromium is not installed the
//! tests report that and pass.
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
use penwise_core::marks::{Binding, BindingMode, MarkItem, MarkStatus, mark_of};
use penwise_core::sample::Sample;
use penwise_proto::{ClientKind, ItemRef, PageId};
use penwise_server::serve::{ServerConfig, ServerHandle, serve};
use penwise_testkit::{DeviceConfig, IdGen, TempDir, TestDevice, system_clock};
use serde_json::{Value, json};

mod common;

use common::{Mcp, TOKEN, png_size};

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

fn item_id(text: &str) -> ItemId {
    ItemId::from_hex(text)
        .or_else(|| text.parse::<ItemRef>().ok().map(ItemId::from))
        .unwrap_or_else(|| panic!("item id {text:?} parses"))
}

/// A stroke whose samples are binding-local: a small box inside the bound origin.
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
            params: BrushParams::for_preset(0, 0xFFE0_3131, 1.0),
            seed,
            samples,
            trim: None,
            binding: Some(binding),
        }),
    }
}

struct Rig {
    server: ServerHandle,
    mcp: Mcp,
    tablet: TestDevice,
    _data: TempDir,
    _site: TempDir,
}

async fn rig(port_base: u16, seed: u64) -> Rig {
    let data = TempDir::new("mcp-round-data").expect("data dir");
    let site = TempDir::new("mcp-round-site").expect("site dir");
    std::fs::write(site.path().join("index.html"), PAGE).expect("index.html");
    std::fs::write(site.path().join("styles.css"), CSS).expect("styles.css");
    let mut config = ServerConfig::new(data.path(), TOKEN);
    config.ocr_url = String::new();
    config.project_port_base = port_base + (std::process::id() % 1_000) as u16;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let server = serve(listener, config).await.expect("server starts");
    let reply = reqwest::Client::new()
        .post(format!("http://{}/projects", server.addr))
        .json(&json!({ "id": "prj_site", "name": "site", "root": site.path(), "static": site.path() }))
        .send()
        .await
        .expect("link project");
    assert!(
        reply.status().is_success(),
        "{}",
        reply.text().await.unwrap()
    );
    let mcp = Mcp::start_for(server.addr.port(), "prj_site");
    mcp.rpc("initialize", json!({})).await;
    let mut tablet = TestDevice::new(
        DeviceConfig {
            url: format!("ws://{}/sync", server.addr),
            token: TOKEN.to_owned(),
            kind: ClientKind::Tablet,
            clock: system_clock(),
        },
        seed,
    );
    tablet.online().await.expect("tablet online");
    Rig {
        server,
        mcp,
        tablet,
        _data: data,
        _site: site,
    }
}

impl Rig {
    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.server.addr)
    }

    /// The owner draws one box mark on `frame` and the tablet settles.
    async fn owner_marks(&mut self, page: PageId, frame: ItemId, ids: &mut IdGen) -> ItemId {
        let (fx, fy, z) = {
            let scene = self.tablet.replica.page(page).expect("page").scene();
            let item = scene.get(frame).expect("frame item");
            (
                item.transform.e,
                item.transform.f,
                scene.next_z().unwrap_or(Z_STEP),
            )
        };
        let binding = Binding::new(
            frame,
            BindingMode::Area,
            FramePreset::Desktop,
            None,
            String::new(),
            [fx, fy],
        )
        .expect("binding");
        let (stroke, mark) = (ids.next_id_item(), ids.next_id_item());
        self.tablet.push_edits(
            page,
            vec![
                Edit::Add(bound_stroke(stroke, z, 1, binding)),
                Edit::Add(
                    MarkItem {
                        frame,
                        preset: FramePreset::Desktop,
                        status: MarkStatus::New,
                        state: String::new(),
                        strokes: vec![stroke],
                    }
                    .item(mark, z + 1),
                ),
            ],
        );
        self.tablet
            .settle(Duration::from_secs(10))
            .await
            .expect("settle");
        mark
    }

    async fn owner_sends(&self) -> Value {
        let reply = reqwest::Client::new()
            .post(self.url("/projects/prj_site/rounds/send"))
            .send()
            .await
            .expect("send");
        assert_eq!(reply.status(), reqwest::StatusCode::OK);
        reply.json().await.expect("send json")
    }

    async fn inbox(&self) -> Value {
        reqwest::get(self.url("/projects/prj_site/inbox"))
            .await
            .expect("inbox")
            .json::<Value>()
            .await
            .expect("inbox json")["line"]
            .clone()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_whole_round_costs_the_agent_four_tool_calls() {
    if !chromium_installed() {
        eprintln!("chromium is not installed; MCP round test skipped");
        return;
    }
    let mut rig = rig(27_000, 21).await;

    // The agent opens the frame (before the owner does anything).
    let opened = rig
        .mcp
        .json("show", json!({ "frame": { "route": "/" } }))
        .await;
    let page: PageId = opened["page"].as_str().unwrap().parse().expect("page id");
    let frame = item_id(opened["frame"].as_str().unwrap());
    assert!(
        rig.tablet
            .pump_until(Duration::from_secs(10), |d| d
                .replica
                .page(page)
                .is_some_and(|p| p.scene().get(frame).is_some()))
            .await,
        "the tablet sees the frame"
    );
    let (frame_x, frame_y) = {
        let scene = rig.tablet.replica.page(page).unwrap().scene();
        let item = scene.get(frame).unwrap();
        (item.transform.e, item.transform.f)
    };

    // The owner marks the frame and taps Send.
    let mut ids = IdGen::new(42);
    let mark = rig.owner_marks(page, frame, &mut ids).await;
    assert_eq!(
        rig.inbox().await,
        Value::Null,
        "nothing is announced before Send"
    );
    assert_eq!(rig.owner_sends().await, json!({ "n": 1, "marks": 1 }));
    let line = rig.inbox().await;
    assert_eq!(
        line,
        "Penwise: round 1 sent \u{2014} 1 mark on Desktop / (call look)"
    );

    let calls_before = rig.mcp.tool_calls();

    // 1. look: the text and one numbered overview image.
    let reply = rig.mcp.call("look", json!({})).await;
    let content = reply["result"]["content"].as_array().unwrap();
    let text = content[0]["text"].as_str().unwrap();
    assert!(text.contains("round 1 sent"), "{text}");
    assert!(
        text.contains(&mark.to_hex()),
        "the mark id is in the text: {text}"
    );
    assert!(
        text.contains("red/problem") || text.contains(" ink "),
        "{text}"
    );
    assert!(
        text.contains("hero"),
        "the mark resolves to the hero: {text}"
    );
    assert!(text.contains("index.html:"), "with file:line: {text}");
    assert!(text.contains("styles.css:"), "{text}");
    assert_eq!(content.len(), 2, "one overview image for the one frame");
    assert_eq!(content[1]["type"], "image");
    let (width, height) = png_size(content[1]["data"].as_str().unwrap());
    assert!(width.max(height) <= 1280, "{width}x{height}");
    assert!(
        width >= 96 && height >= 40,
        "the crop holds the mark and its margin"
    );
    assert_eq!(rig.inbox().await, Value::Null, "looking silences the inbox");
    {
        // Returned marks moved new -> seen.
        let marks: Value = reqwest::get(rig.url(&format!("/pages/{page}/marks?status=seen")))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(marks["marks"].as_array().unwrap().len(), 1, "{marks}");
    }

    // 2. show variants: three, beside the frame.
    let created = rig
        .mcp
        .json(
            "show",
            json!({ "variants": { "frame": opened["frame"], "options": [
                { "label": "calm", "patch": { "css": ".hero { color: gray }" } },
                { "label": "bold", "patch": { "css": ".hero { font-weight: 900 }" } },
                { "label": "short", "patch": { "dom": [{ "op": "set_text", "selector": ".hero", "value": "Hi" }] } }
            ] } }),
        )
        .await;
    let variants: Vec<ItemId> = created["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| item_id(id.as_str().unwrap()))
        .collect();
    assert_eq!(variants.len(), 3, "{created}");

    // The owner picks "bold" ("Use this" is a FrameChosen edit from the tablet).
    assert!(
        rig.tablet
            .pump_until(Duration::from_secs(10), |d| {
                let scene = d.replica.page(page).unwrap().scene();
                variants.iter().all(|id| scene.get(*id).is_some())
            })
            .await,
        "the tablet sees the variants"
    );
    let edits = {
        let scene = rig.tablet.replica.page(page).unwrap().scene();
        penwise_core::item::set_chosen(scene, variants[1], true).expect("set_chosen")
    };
    rig.tablet.push_edits(page, edits);
    rig.tablet
        .settle(Duration::from_secs(10))
        .await
        .expect("settle");

    // 3. look: the pick is in the text.
    let reply = rig.mcp.call("look", json!({ "images": "none" })).await;
    let content = reply["result"]["content"].as_array().unwrap();
    assert_eq!(content.len(), 1, "images: none is text only");
    let text = content[0]["text"].as_str().unwrap();
    assert!(
        text.contains(&format!("picked \"bold\" {}", variants[1].to_hex())),
        "{text}"
    );

    // 4. resolve the round.
    let resolved = rig.mcp.json("resolve", json!({ "round": 1 })).await;
    assert_eq!(resolved["round"]["status"], "resolved", "{resolved}");
    assert_eq!(resolved["round"]["snapshots"].as_array().unwrap().len(), 1);
    assert_eq!(resolved["round"]["variants_removed"], 3);

    assert_eq!(
        rig.mcp.tool_calls() - calls_before,
        4,
        "look, show, look, resolve"
    );

    // The board afterwards: history picture where the frame stood, the frame one column right, variants gone,
    // the mark done.
    let images: Value = reqwest::Client::new()
        .get(rig.url(&format!("/pages/{page}/images")))
        .header("X-Penwise-Token", TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let pictures = images["images"].as_array().unwrap();
    assert_eq!(pictures.len(), 1, "{images}");
    assert_eq!(pictures[0]["label"], "Round 1 \u{2014} desktop /");
    assert_eq!(pictures[0]["x"].as_f64(), Some(f64::from(frame_x)));
    assert_eq!(pictures[0]["y"].as_f64(), Some(f64::from(frame_y)));
    assert_eq!(pictures[0]["w"].as_f64(), Some(1440.0));
    assert!(pictures[0]["h"].as_f64().unwrap() >= 900.0);
    assert!(
        rig.tablet
            .pump_until(Duration::from_secs(10), |d| {
                let scene = d.replica.page(page).unwrap().scene();
                variants.iter().all(|id| scene.get(*id).is_none())
                    && mark_of(scene, mark).is_some_and(|m| m.status == MarkStatus::Done)
            })
            .await,
        "the tablet sees the variants gone and the mark done"
    );
    let scene = rig.tablet.replica.page(page).unwrap().scene();
    let moved = scene.get(frame).expect("live frame");
    assert!(
        (moved.transform.e - (frame_x + 1440.0 + 40.0)).abs() < 0.01,
        "one column right: {} from {frame_x}",
        moved.transform.e
    );
    assert!((moved.transform.f - frame_y).abs() < 0.01, "same row");
    let ItemKind::Frame(live) = &moved.kind else {
        panic!("not a frame");
    };
    assert!(live.patch.is_none() && live.group.is_none());

    // The round list agrees; the next round is open and empty.
    let rounds: Value = reqwest::get(rig.url("/projects/prj_site/rounds"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(rounds["rounds"][0]["status"], "resolved");
    assert_eq!(rounds["rounds"][1]["n"], 2);
    assert_eq!(rounds["rounds"][1]["marks"], 0);
    rig.server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_round_closes_by_itself_when_its_last_open_mark_is_done() {
    if !chromium_installed() {
        eprintln!("chromium is not installed; MCP auto-resolve test skipped");
        return;
    }
    let mut rig = rig(28_000, 22).await;
    let opened = rig
        .mcp
        .json("show", json!({ "frame": { "route": "/" } }))
        .await;
    let page: PageId = opened["page"].as_str().unwrap().parse().unwrap();
    let frame = item_id(opened["frame"].as_str().unwrap());
    assert!(
        rig.tablet
            .pump_until(Duration::from_secs(10), |d| d
                .replica
                .page(page)
                .is_some_and(|p| p.scene().get(frame).is_some()))
            .await
    );
    let mut ids = IdGen::new(43);
    let first = rig.owner_marks(page, frame, &mut ids).await;
    let second = rig.owner_marks(page, frame, &mut ids).await;
    assert_eq!(rig.owner_sends().await, json!({ "n": 1, "marks": 2 }));

    // One of two marks done: the round stays open for the agent.
    let partial = rig
        .mcp
        .json(
            "resolve",
            json!({ "marks": [{ "id": first.to_hex(), "status": "done", "note": "fixed" }] }),
        )
        .await;
    assert_eq!(partial["marks"][0]["status"], "done");
    assert_eq!(partial["round"], Value::Null, "{partial}");

    // The last one: the round resolves without `round: true`.
    let last = rig
        .mcp
        .json(
            "resolve",
            json!({ "marks": [{ "id": second.to_hex(), "status": "done" }] }),
        )
        .await;
    assert_eq!(last["round"]["status"], "resolved", "{last}");
    assert_eq!(last["round"]["snapshots"].as_array().unwrap().len(), 1);
    rig.server.shutdown().await;
}

/// The frames of `page` the tablet knows, `(x, y)` by id.
fn position(rig: &Rig, page: PageId, id: ItemId) -> (f32, f32) {
    let scene = rig.tablet.replica.page(page).expect("page").scene();
    let item = scene
        .get(id)
        .unwrap_or_else(|| panic!("{id:?} is on the board"));
    (item.transform.e, item.transform.f)
}

async fn show_frame(rig: &Rig, arguments: Value) -> (PageId, ItemId) {
    let shown = rig.mcp.json("show", json!({ "frame": arguments })).await;
    (
        shown["page"].as_str().unwrap().parse().unwrap(),
        item_id(shown["frame"].as_str().unwrap()),
    )
}

async fn wait_for(rig: &mut Rig, page: PageId, frames: &[ItemId]) {
    let ids = frames.to_vec();
    assert!(
        rig.tablet
            .pump_until(Duration::from_secs(10), |d| d
                .replica
                .page(page)
                .is_some_and(|p| ids.iter().all(|id| p.scene().get(*id).is_some())))
            .await,
        "the tablet sees the frames"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resolving_makes_room_in_the_frames_row_and_leaves_other_lanes_alone() {
    if !chromium_installed() {
        eprintln!("chromium is not installed; MCP lanes test skipped");
        return;
    }
    let mut rig = rig(29_000, 23).await;
    // Three screens, each in a lane of its own; then the owner drags the second route into the first one's row.
    let (page, home) = show_frame(&rig, json!({ "route": "/" })).await;
    let (_, work) = show_frame(&rig, json!({ "route": "/work" })).await;
    let (_, phone) = show_frame(&rig, json!({ "route": "/", "preset": "phone" })).await;
    wait_for(&mut rig, page, &[home, work, phone]).await;
    let home_at = position(&rig, page, home);
    let beside = Affine::translate(home_at.0 + 1440.0 + 40.0, home_at.1);
    rig.tablet.push_edits(
        page,
        vec![Edit::Update {
            id: work,
            patch: penwise_core::edit::ItemPatch {
                transform: Some(beside),
                ..penwise_core::edit::ItemPatch::default()
            },
        }],
    );
    rig.tablet
        .settle(Duration::from_secs(10))
        .await
        .expect("settle");
    let (home_before, work_before, phone_before) = (
        position(&rig, page, home),
        position(&rig, page, work),
        position(&rig, page, phone),
    );
    assert!(
        work_before.0 > home_before.0,
        "the second route is right of the first"
    );
    assert!(
        (work_before.1 - home_before.1).abs() < 0.01,
        "on the same row"
    );
    assert!(
        phone_before.1 > home_before.1,
        "the phone frame is in a lane below"
    );

    let mut ids = IdGen::new(44);
    let mark = rig.owner_marks(page, home, &mut ids).await;
    assert_eq!(rig.owner_sends().await, json!({ "n": 1, "marks": 1 }));
    let resolved = rig.mcp.json("resolve", json!({ "round": 1 })).await;
    assert_eq!(resolved["round"]["status"], "resolved", "{resolved}");

    assert!(
        rig.tablet
            .pump_until(Duration::from_secs(10), |d| {
                let scene = d.replica.page(page).unwrap().scene();
                mark_of(scene, mark).is_some_and(|m| m.status == MarkStatus::Done)
                    && scene
                        .get(home)
                        .is_some_and(|i| i.transform.e > home_before.0 + 1.0)
            })
            .await,
        "the tablet sees the resolve"
    );
    let column = 1440.0 + 40.0;
    let (home_after, work_after, phone_after) = (
        position(&rig, page, home),
        position(&rig, page, work),
        position(&rig, page, phone),
    );
    assert!(
        (home_after.0 - (home_before.0 + column)).abs() < 0.01,
        "the frame moved one column"
    );
    assert!(
        (work_after.0 - (work_before.0 + column)).abs() < 0.01,
        "the next route moved out of the way: {work_before:?} -> {work_after:?}"
    );
    assert!(
        home_after.0 + 1440.0 <= work_after.0 + 0.01,
        "the live frame does not overlap the next route"
    );
    assert_eq!(phone_after, phone_before, "the other lane did not move");
    rig.server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_open_round_needs_its_number_and_two_resolves_freeze_once() {
    if !chromium_installed() {
        eprintln!("chromium is not installed; MCP resolve-number test skipped");
        return;
    }
    let mut rig = rig(30_000, 24).await;
    let (page, frame) = show_frame(&rig, json!({ "route": "/" })).await;
    wait_for(&mut rig, page, &[frame]).await;
    let mut ids = IdGen::new(45);
    let first = rig.owner_marks(page, frame, &mut ids).await;

    // The round is still open (never sent): `round: true` refuses and changes nothing.
    let refused = rig.mcp.failure("resolve", json!({ "round": true })).await;
    assert!(refused.contains("no sent round"), "{refused}");
    let rounds: Value = reqwest::get(rig.url("/projects/prj_site/rounds"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(rounds["rounds"][0]["status"], "open", "{rounds}");

    // Naming the open round resolves it, twice at once; the second finds it done.
    let url = rig.url("/projects/prj_site/rounds/1/resolve");
    let client = reqwest::Client::new();
    let (a, b) = tokio::join!(client.post(&url).send(), client.post(&url).send());
    let (a, b): (Value, Value) = (
        a.unwrap().json().await.unwrap(),
        b.unwrap().json().await.unwrap(),
    );
    let frozen = [&a, &b]
        .iter()
        .filter(|reply| reply["snapshots"].as_array().is_some_and(|s| s.len() == 1))
        .count();
    let already = [&a, &b]
        .iter()
        .filter(|reply| reply["already"] == true)
        .count();
    assert_eq!((frozen, already), (1, 1), "{a} / {b}");
    let images: Value = reqwest::Client::new()
        .get(rig.url(&format!("/pages/{page}/images")))
        .header("X-Penwise-Token", TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        images["images"].as_array().unwrap().len(),
        1,
        "one picture: {images}"
    );

    // The round was sent before it froze: a mark drawn now is round 2's, still new.
    let late = rig.owner_marks(page, frame, &mut ids).await;
    let rounds: Value = reqwest::get(rig.url("/projects/prj_site/rounds"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(rounds["rounds"][0]["status"], "resolved");
    assert_eq!(rounds["rounds"][1]["marks"], 1);
    assert_eq!(rounds["rounds"][1]["new"], 1);
    let scene = rig.tablet.replica.page(page).unwrap().scene();
    assert_eq!(mark_of(scene, first).unwrap().status, MarkStatus::Done);
    assert_eq!(mark_of(scene, late).unwrap().status, MarkStatus::New);
    rig.server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn marks_drawn_after_send_wait_for_the_next_round() {
    if !chromium_installed() {
        eprintln!("chromium is not installed; MCP since-send test skipped");
        return;
    }
    let mut rig = rig(31_000, 25).await;
    let (page, frame) = show_frame(&rig, json!({ "route": "/" })).await;
    wait_for(&mut rig, page, &[frame]).await;
    let mut ids = IdGen::new(46);
    let first = rig.owner_marks(page, frame, &mut ids).await;
    assert_eq!(rig.owner_sends().await, json!({ "n": 1, "marks": 1 }));

    // Variants beside the frame; the owner picks one and marks it, and marks the frame again: both after Send.
    let created = rig
        .mcp
        .json(
            "show",
            json!({ "variants": { "frame": frame.to_hex(), "options": [
                { "label": "calm", "patch": { "css": ".hero { color: gray }" } },
                { "label": "bold", "patch": { "css": ".hero { font-weight: 900 }" } }
            ] } }),
        )
        .await;
    let variants: Vec<ItemId> = created["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| item_id(id.as_str().unwrap()))
        .collect();
    wait_for(&mut rig, page, &variants).await;
    let edits = {
        let scene = rig.tablet.replica.page(page).unwrap().scene();
        penwise_core::item::set_chosen(scene, variants[1], true).unwrap()
    };
    rig.tablet.push_edits(page, edits);
    let on_variant = rig.owner_marks(page, variants[1], &mut ids).await;
    let on_frame = rig.owner_marks(page, frame, &mut ids).await;

    // look: the round's mark first, the late ones apart.
    let reply = rig.mcp.call("look", json!({ "images": "none" })).await;
    let text = reply["result"]["content"][0]["text"].as_str().unwrap();
    let (round, since) = text
        .split_once("since send (next round):")
        .expect("a since-send section");
    assert!(round.contains(&first.to_hex()), "{text}");
    assert!(
        since.contains(&on_frame.to_hex()) && since.contains(&on_variant.to_hex()),
        "{text}"
    );
    assert!(!round.contains(&on_frame.to_hex()), "{text}");

    // Resolving round 1 freezes round 1 only: the late marks stay new, the variant that carries one stays.
    let resolved = rig.mcp.json("resolve", json!({ "round": 1 })).await;
    assert_eq!(resolved["round"]["status"], "resolved", "{resolved}");
    assert_eq!(resolved["round"]["variants_removed"], 1, "{resolved}");
    assert!(
        rig.tablet
            .pump_until(Duration::from_secs(10), |d| {
                let scene = d.replica.page(page).unwrap().scene();
                scene.get(variants[0]).is_none()
                    && mark_of(scene, first).is_some_and(|m| m.status == MarkStatus::Done)
            })
            .await
    );
    let scene = rig.tablet.replica.page(page).unwrap().scene();
    assert!(
        scene.get(variants[1]).is_some(),
        "the variant with a newer mark is kept"
    );
    assert_eq!(mark_of(scene, on_variant).unwrap().status, MarkStatus::New);
    assert_eq!(mark_of(scene, on_frame).unwrap().status, MarkStatus::New);
    rig.server.shutdown().await;
}
