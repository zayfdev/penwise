//! Agent bundle tests (C2): the bundle against the harness fixture in a real headless Chromium, its crops,
//! checkpoints, `frames/open` and the frame snapshot.
//!
//! Chromium is the feature: without `/usr/bin/chromium` (or `PENWISE_CHROMIUM`) the headless parts are skipped
//! with a message, exactly like `tests/inspect.rs`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use std::path::PathBuf;
use std::sync::Arc;

use penwise_core::brush::BrushParams;
use penwise_core::edit::Edit;
use penwise_core::item::{Item, ItemId, ItemKind, Layer, StrokeItem};
use penwise_core::marks::{Binding, BindingMode, MarkItem, MarkStatus};
use penwise_core::sample::Sample;
use penwise_proto::{AnyOp, ItemRef, Op, OpId, PageId, PageKind};
use serde_json::{Value, json};

use crate::api::projects::routes as group;
use crate::frames::registry::LinkRequest;
use crate::testing::{Rig, T0};

const HTML: &str = include_str!("../../tests/fixtures/harness/index.html");
const CSS: &str = include_str!("../../tests/fixtures/harness/styles.css");

/// The binding key the injected bridge computes for the fixture's `#mover` (`v1|<data-pw-src>|fingerprint`).
const MOVER_KEY: &str = "v1||div#mover.box:nth-of-type(1)";
/// Red ink (ARGB), the "problem" colour.
const RED: u32 = 0xFFFF_0000;

fn chromium() -> bool {
    std::env::var_os("PENWISE_CHROMIUM")
        .map_or_else(|| PathBuf::from("/usr/bin/chromium"), PathBuf::from)
        .is_file()
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/harness")
}

fn line_of(text: &str, needle: &str) -> u64 {
    u64::try_from(
        text.lines()
            .position(|line| line.contains(needle))
            .expect("needle is in the fixture")
            + 1,
    )
    .expect("line fits")
}

fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(
        &png[..8],
        &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
        "png signature"
    );
    (
        u32::from_be_bytes([png[16], png[17], png[18], png[19]]),
        u32::from_be_bytes([png[20], png[21], png[22], png[23]]),
    )
}

/// A link of the harness fixture and a server serving the project + agent routes on a loopback port.
async fn fixture() -> (Rig, String) {
    let rig = Rig::new();
    let root = fixture_root();
    let registry = rig.projects();
    registry
        .link(LinkRequest {
            id: Some("prj_harness".to_owned()),
            name: Some("harness".to_owned()),
            root: root.clone(),
            folder: Some(root),
            origin: None,
        })
        .await
        .expect("fixture project links");
    let base = rig
        .serve_app(
            group(Arc::new(String::new())),
            rig.app_with_projects(registry),
        )
        .await;
    (rig, base)
}

async fn post(url: &str, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(url)
        .json(&body)
        .send()
        .await
        .expect("post")
}

async fn json(url: &str) -> Value {
    let response = reqwest::get(url).await.expect("get");
    assert_eq!(
        response.status(),
        reqwest::StatusCode::OK,
        "{url}: {}",
        response.text().await.expect("body")
    );
    response.json().await.expect("json")
}

/// A closed loop in binding-local coordinates (what the classifier reads as a circle).
fn loop_points() -> Vec<[f32; 2]> {
    (0..=16)
        .map(|step| {
            #[allow(clippy::cast_precision_loss, reason = "16 steps")]
            let angle = step as f32 / 16.0 * std::f32::consts::TAU;
            [40.0 + 30.0 * angle.cos(), 30.0 + 26.0 * angle.sin()]
        })
        .collect()
}

/// A bound stroke of red ink, in binding-local coordinates.
fn bound_stroke(id: ItemId, z: i64, binding: Binding) -> Item {
    let samples = loop_points()
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
        transform: penwise_core::item::Affine::IDENTITY,
        kind: ItemKind::Stroke(StrokeItem {
            params: BrushParams::for_preset(0, RED, 1.0),
            seed: 7,
            samples,
            trim: None,
            binding: Some(binding),
        }),
    }
}

/// A phone frame on `page` through the public route; returns the frame id.
async fn create_frame(base: &str, page: PageId, x: f32, y: f32) -> ItemId {
    let body = json!({
        "project": "prj_harness",
        "route": "/",
        "preset": "phone",
        "x": x,
        "y": y
    });
    let response = post(&format!("{base}/pages/{page}/frames"), body).await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let value: Value = response.json().await.expect("frame json");
    let text = value["id"].as_str().expect("frame id");
    ItemId::from_hex(text)
        .or_else(|| text.parse::<ItemRef>().ok().map(ItemId::from))
        .expect("frame id parses")
}

/// Appends `edits` as a device's ops with fresh server stamps: later than every op so far, exactly how a
/// `FrameChosen` edit arrives from the tablet.
async fn device_edits(rig: &Rig, page: PageId, edits: Vec<Edit>) {
    let device = rig.storage.server_device();
    let mut ops = Vec::with_capacity(edits.len());
    for (index, edit) in edits.into_iter().enumerate() {
        let stamp = rig.storage.server_stamp().await.expect("stamp");
        #[allow(clippy::cast_possible_truncation, reason = "a handful of edits")]
        let offset = index as u64;
        ops.push(AnyOp::Item(Box::new(Op {
            op_id: OpId::from_parts(T0, 500 + u128::from(offset)),
            device,
            hlc: stamp,
            page,
            edit,
        })));
    }
    rig.storage
        .append_ops(device, ops)
        .await
        .expect("device ops");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_bundle_names_the_element_its_source_its_kind_colour_and_crops() {
    if !chromium() {
        eprintln!("skipping: no chromium at /usr/bin/chromium (set PENWISE_CHROMIUM)");
        return;
    }
    let (mut rig, base) = fixture().await;
    let page = rig.fixed_page().await;
    let frame = create_frame(&base, page, 10.0, 20.0).await;

    // A tablet draws a red loop around the fixture's `#mover`, bound to its key.
    let binding = Binding::new(
        frame,
        BindingMode::Bound,
        penwise_core::item::FramePreset::Phone,
        Some(MOVER_KEY.to_owned()),
        String::new(),
        [10.0, 20.0],
    )
    .expect("binding");
    let stroke = rig.item_id();
    let mark = rig.item_id();
    rig.edit(page, Edit::Add(bound_stroke(stroke, 1, binding)))
        .await;
    rig.edit(
        page,
        Edit::Add(
            MarkItem {
                frame,
                preset: penwise_core::item::FramePreset::Phone,
                status: MarkStatus::New,
                state: String::new(),
                strokes: vec![stroke],
            }
            .item(mark, 2),
        ),
    )
    .await;

    let body = json(&format!("{base}/marks?project=prj_harness&agent=test")).await;
    let checkpoint = body["checkpoint"].as_u64().expect("checkpoint");
    let marks = body["marks"].as_array().expect("marks");
    assert_eq!(marks.len(), 1, "{body}");
    let bundle = &marks[0];
    assert_eq!(bundle["id"], mark.to_hex());
    assert_eq!(bundle["frame"], frame.to_hex());
    assert_eq!(bundle["project"], "prj_harness");
    assert_eq!(bundle["route"], "/");
    assert_eq!(bundle["preset"], "phone");
    assert_eq!(bundle["binding"], "bound");
    assert_eq!(bundle["status"], "new");
    assert_eq!(bundle["kind"]["guess"], "circle", "{bundle}");
    assert_eq!(bundle["colour"], "problem", "{bundle}");
    assert_eq!(bundle["text"], Value::Null);
    assert_eq!(
        bundle["text_status"], "off",
        "OCR is not configured in the rig"
    );

    // The element, its HTML line and its matched CSS rule.
    let element = &bundle["element"];
    assert_eq!(element["selector"], "#mover", "{element}");
    assert!(element["role"].is_string(), "AX role: {element}");
    assert!(
        element["text"].as_str().expect("text").contains("mover"),
        "{element}"
    );
    let sources = element["source"].as_array().expect("source");
    let html_line = line_of(HTML, "<div id=\"mover\"");
    assert!(
        sources
            .iter()
            .any(|source| source["file"] == "index.html" && source["line"] == html_line),
        "{sources:#?}"
    );
    let css_line = line_of(CSS, ".box {");
    assert!(
        sources
            .iter()
            .any(|source| source["file"] == "styles.css" && source["line"] == css_line),
        "{sources:#?}"
    );

    // The crops: served, sized from the mark rect, and the ink really is in with_ink.
    let rect = bundle["rect"]
        .as_array()
        .expect("rect")
        .iter()
        .map(|value| value.as_f64().expect("rect number"))
        .collect::<Vec<_>>();
    assert_eq!(rect.len(), 4, "{bundle}");
    let images = &bundle["images"];
    let url = |kind: &str| {
        images[kind]
            .as_str()
            .expect(kind)
            .strip_prefix('/')
            .map(|path| format!("{base}/{path}"))
            .expect("relative URL")
    };
    let with_ink = reqwest::get(url("with_ink")).await.expect("with_ink");
    assert_eq!(with_ink.status(), reqwest::StatusCode::OK);
    assert_eq!(with_ink.headers()["content-type"], "image/png");
    let with_ink = with_ink.bytes().await.expect("bytes").to_vec();
    let without_ink = reqwest::get(url("without_ink"))
        .await
        .expect("without_ink")
        .bytes()
        .await
        .expect("bytes")
        .to_vec();
    let context = reqwest::get(url("context"))
        .await
        .expect("context")
        .bytes()
        .await
        .expect("bytes")
        .to_vec();
    assert_eq!(png_size(&context), (390, 844), "the frame viewport");
    let (w, h) = png_size(&with_ink);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "fixture-sized positive rects"
    )]
    let expected = (
        (rect[2] + 96.0).ceil() as u32,
        (rect[3] + 96.0).ceil() as u32,
    );
    assert!(
        w.abs_diff(expected.0) <= 2 && h.abs_diff(expected.1) <= 2,
        "crop {w}x{h} around {rect:?} (expected ~{expected:?})"
    );
    assert_eq!(png_size(&without_ink), (w, h));
    assert_ne!(with_ink, without_ink, "the ink is composited on the crop");

    // `since=last`: nothing the second time, the mark again after its status changes.
    let again = json(&format!(
        "{base}/marks?project=prj_harness&agent=test&since=last"
    ))
    .await;
    assert_eq!(again["marks"], json!([]), "{again}");
    assert!(
        again["checkpoint"].as_u64().expect("checkpoint") >= checkpoint,
        "checkpoints never move backwards"
    );
    let response = post(
        &format!("{base}/marks/{}/status", mark.to_hex()),
        json!({ "status": "reopened" }),
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let reopened = json(&format!(
        "{base}/marks?project=prj_harness&agent=test&since=last"
    ))
    .await;
    assert_eq!(
        reopened["marks"].as_array().expect("marks").len(),
        1,
        "{reopened}"
    );
    assert_eq!(reopened["marks"][0]["status"], "reopened");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn frames_open_creates_the_review_page_side_by_side_and_snapshots_png() {
    if !chromium() {
        eprintln!("skipping: no chromium at /usr/bin/chromium (set PENWISE_CHROMIUM)");
        return;
    }
    let (rig, base) = fixture().await;
    let open = |preset: &str| {
        let url = format!("{base}/frames/open");
        let body = json!({ "project": "prj_harness", "route": "/", "preset": preset });
        async move {
            let response = post(&url, body).await;
            assert_eq!(response.status(), reqwest::StatusCode::OK);
            response.json::<Value>().await.expect("open json")
        }
    };
    let first = open("phone").await;
    let second = open("desktop").await;
    assert_eq!(first["page"], second["page"], "one review page");
    let page: PageId = first["page"]
        .as_str()
        .expect("page")
        .parse()
        .expect("page id");
    let first_frame = ItemId::from_hex(first["frame"].as_str().expect("frame")).expect("hex");
    let second_frame = ItemId::from_hex(second["frame"].as_str().expect("frame")).expect("hex");

    // The review page lives in the project's notebook, titled "<project> board".
    let catalog = rig.storage.catalog().await.expect("catalog");
    let review = catalog.page(page).expect("review page");
    assert_eq!(review.title, "harness board");
    let project = rig
        .storage
        .catalog()
        .await
        .expect("catalog")
        .notebooks()
        .into_iter()
        .find(|notebook| notebook.title == "harness")
        .expect("project notebook");
    assert_eq!(review.notebook, project.id);

    // Each screen (route + preset) is a lane. A new screen starts a new lane at the board's left edge,
    // 200 px under the lowest box; it never goes to the right of another lane's frame.
    let view = rig.view(page).await;
    let scene = view.replica.scene();
    let phone = scene.get(first_frame).expect("phone frame");
    let desktop = scene.get(second_frame).expect("desktop frame");
    assert_eq!((phone.transform.e, phone.transform.f), (0.0, 0.0));
    assert_eq!(
        (desktop.transform.e, desktop.transform.f),
        (0.0, 844.0 + 200.0),
        "a second preset of the route is a new lane under the phone's box"
    );

    // Another route is another screen: a third lane under the desktop box (1044 + 900 + 200).
    let third = post(
        &format!("{base}/frames/open"),
        json!({ "project": "prj_harness", "route": "/other.html" }),
    )
    .await;
    assert_eq!(third.status(), reqwest::StatusCode::OK);
    let third: Value = third.json().await.expect("third json");
    let third_frame = ItemId::from_hex(third["frame"].as_str().expect("frame")).expect("hex");
    let view = rig.view(page).await;
    let scene = view.replica.scene();
    let other = scene.get(third_frame).expect("third frame");
    assert_eq!(
        (other.transform.e, other.transform.f),
        (0.0, 1044.0 + 900.0 + 200.0),
        "a new screen goes in a new lane, never right of another lane's frame"
    );

    // The same screen again continues its own lane: right of its rightmost frame, on its row.
    let again = post(
        &format!("{base}/frames/open"),
        json!({ "project": "prj_harness", "route": "/", "preset": "phone" }),
    )
    .await;
    let again: Value = again.json().await.expect("again json");
    let again_frame = ItemId::from_hex(again["frame"].as_str().expect("frame")).expect("hex");
    let view = rig.view(page).await;
    let beside = view.replica.scene().get(again_frame).expect("phone again");
    assert_eq!(
        (beside.transform.e, beside.transform.f),
        (390.0 + 40.0, 0.0),
        "same screen, same lane, one gap to the right"
    );
    let _ = other;
    let other = view.replica.scene().get(third_frame).expect("third frame");
    match &other.kind {
        ItemKind::Frame(frame) => {
            assert_eq!(
                frame.preset,
                penwise_core::item::FramePreset::Desktop,
                "preset defaults to desktop"
            );
            assert_eq!(frame.extent, penwise_core::item::FrameExtent::Full);
        }
        other => panic!("not a frame: {other:?}"),
    }

    // A snapshot is the frame at its preset size.
    let response = reqwest::get(format!(
        "{base}/frames/{}/snapshot.png?ink=true",
        first_frame.to_hex()
    ))
    .await
    .expect("snapshot");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/png");
    let png = response.bytes().await.expect("bytes");
    // A frame opens full length: the snapshot is the whole document at the preset width, never shorter than
    // the viewport (the fixture page is taller than a phone screen).
    let (width, height) = png_size(&png);
    assert_eq!(width, 390);
    assert!(
        height > 844,
        "the whole document, not the first screen: {height}"
    );

    let plain = reqwest::get(format!(
        "{base}/frames/{}/snapshot.png?ink=false",
        second_frame.to_hex()
    ))
    .await
    .expect("snapshot");
    assert_eq!(plain.status(), reqwest::StatusCode::OK);
    let (width, height) = png_size(&plain.bytes().await.expect("bytes"));
    assert_eq!(width, 1440);
    assert!(height >= 900, "never shorter than the viewport: {height}");
}

/* ------------------------------------------------------------------ ghosts and variants (M4, P4) */

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ghost_sets_clears_and_reports_failed_selectors() {
    if !chromium() {
        eprintln!("skipping: no chromium at /usr/bin/chromium (set PENWISE_CHROMIUM)");
        return;
    }
    let (mut rig, base) = fixture().await;
    let page = rig.fixed_page().await;
    let frame = create_frame(&base, page, 10.0, 20.0).await;
    let snapshot = || {
        let url = format!("{base}/frames/{}/snapshot.png?ink=false", frame.to_hex());
        async move {
            reqwest::get(url)
                .await
                .expect("snapshot")
                .bytes()
                .await
                .expect("bytes")
                .to_vec()
        }
    };
    let before = snapshot().await;

    let patch = json!({
        "css": "#mover { background: rgb(1, 2, 3) }",
        "dom": [{ "op": "set_text", "selector": "#title", "value": "Ghosted" }]
    });
    let response = post(
        &format!("{base}/frames/{}/ghost", frame.to_hex()),
        json!({ "patch": patch }),
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let result: Value = response.json().await.expect("ghost json");
    assert_eq!(result["ok"], json!(true), "{result}");
    assert_eq!(result["failed"], json!([]));

    // The snapshot is the patched page: the same frame without its patch renders different pixels.
    let patched = snapshot().await;
    assert_ne!(patched, before, "the snapshot shows the patch");
    assert_eq!(png_size(&patched).0, 390);

    // The stored text is the sanitized P1 JSON.
    let view = rig.view(page).await;
    let stored = view
        .replica
        .scene()
        .get(frame)
        .and_then(|item| match &item.kind {
            ItemKind::Frame(frame) => frame.patch.clone(),
            _ => None,
        })
        .expect("the frame keeps its patch");
    assert!(stored.contains("set_text"), "{stored}");

    // A selector matching nothing is reported, and the unsafe parts of the patch never reach the frame.
    let response = post(
        &format!("{base}/frames/{}/ghost", frame.to_hex()),
        json!({ "patch": {
            "dom": [
                { "op": "remove", "selector": ".does-not-exist" },
                { "op": "replace", "selector": "#mover", "html": "<script>bad()</script><b onclick=\"x()\">m</b>" }
            ]
        } }),
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let result: Value = response.json().await.expect("ghost json");
    assert_eq!(result["ok"], json!(false), "{result}");
    assert_eq!(result["failed"][0]["index"], json!(0));
    assert_eq!(result["failed"][0]["reason"], json!("no match"));
    let view = rig.view(page).await;
    let stored = view
        .replica
        .scene()
        .get(frame)
        .and_then(|item| match &item.kind {
            ItemKind::Frame(frame) => frame.patch.clone(),
            _ => None,
        })
        .expect("patch");
    assert!(!stored.contains("script"), "{stored}");
    assert!(!stored.contains("onclick"), "{stored}");

    // `null` clears it, and the frame renders as the code has it again.
    let response = post(
        &format!("{base}/frames/{}/ghost", frame.to_hex()),
        json!({ "patch": Value::Null }),
    )
    .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let view = rig.view(page).await;
    let cleared = view
        .replica
        .scene()
        .get(frame)
        .and_then(|item| match &item.kind {
            ItemKind::Frame(frame) => frame.patch.clone(),
            _ => None,
        });
    assert_eq!(cleared, None);
    assert_eq!(snapshot().await, before, "no patch, the original pixels");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn variants_sit_side_by_side_take_the_pick_and_keep_marks_on_clear() {
    if !chromium() {
        eprintln!("skipping: no chromium at /usr/bin/chromium (set PENWISE_CHROMIUM)");
        return;
    }
    let (mut rig, base) = fixture().await;
    let page = rig.fixed_page().await;
    let frame = create_frame(&base, page, 10.0, 20.0).await;

    let body = json!({ "variants": [
        { "label": "A", "patch": { "css": "#mover { background: red }" } },
        { "label": "B", "patch": { "css": "#mover { background: blue }" } },
        { "label": "C", "patch": { "dom": [{ "op": "set_text", "selector": "#title", "value": "C" }] } }
    ] });
    let response = post(&format!("{base}/frames/{}/variants", frame.to_hex()), body).await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let created: Value = response.json().await.expect("variants json");
    let group = created["group"].as_str().expect("group").to_owned();
    let ids: Vec<ItemId> = created["frames"]
        .as_array()
        .expect("frames")
        .iter()
        .map(|id| ItemId::from_hex(id.as_str().expect("id")).expect("hex"))
        .collect();
    assert_eq!(ids.len(), 3);

    // Side by side: gap 40, same row, same project/route/preset, tagged with the group and labels.
    let view = rig.view(page).await;
    let scene = view.replica.scene();
    let base_x = scene.get(frame).expect("base").transform.e;
    let mut expected_x = base_x + 390.0 + 40.0;
    for (id, label) in ids.iter().zip(["A", "B", "C"]) {
        let item = scene.get(*id).expect("variant");
        assert_eq!(item.transform.e, expected_x, "gap 40");
        assert_eq!(item.transform.f, 20.0);
        let ItemKind::Frame(variant) = &item.kind else {
            panic!("a variant is a frame")
        };
        assert_eq!(variant.label, label);
        assert_eq!(variant.group.as_deref(), Some(group.as_str()));
        assert!(!variant.chosen);
        assert_eq!(variant.project, "prj_harness");
        assert_eq!(variant.route, "/");
        assert_eq!(variant.preset, penwise_core::item::FramePreset::Phone);
        assert!(variant.patch.is_some());
        expected_x += 390.0 + 40.0;
    }

    // The group read before a pick: three labelled frames, no choice.
    let listing = json(&format!("{base}/variants/{group}")).await;
    assert_eq!(listing["group"], group);
    assert_eq!(listing["chosen"], Value::Null);
    let frames = listing["frames"].as_array().expect("frames");
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[1]["label"], "B");
    assert_eq!(frames[1]["patch"]["css"], "#mover { background: blue }");

    // The owner taps "Use this" on the tablet: a device op, exactly what `set_chosen` emits.
    let chosen = ids[1];
    let view = rig.view(page).await;
    let edits =
        penwise_core::item::set_chosen(view.replica.scene(), chosen, true).expect("set_chosen");
    assert_eq!(edits.len(), 1, "nobody else was chosen");
    device_edits(&rig, page, edits).await;
    let view = rig.view(page).await;
    let chosen_now = view
        .replica
        .scene()
        .get(chosen)
        .and_then(|item| match &item.kind {
            ItemKind::Frame(frame) => Some(frame.chosen),
            _ => None,
        });
    assert_eq!(chosen_now, Some(true), "the device pick lands in the scene");
    let picked = json(&format!("{base}/variants/{group}")).await;
    assert_eq!(picked["chosen"], chosen.to_hex());
    assert_eq!(picked["frames"][1]["chosen"], json!(true));
    assert_eq!(picked["frames"][0]["chosen"], json!(false));

    // Ink drawn on a variant is in the group's bundles.
    let binding = Binding::new(
        chosen,
        BindingMode::Bound,
        penwise_core::item::FramePreset::Phone,
        Some(MOVER_KEY.to_owned()),
        String::new(),
        [10.0, 20.0],
    )
    .expect("binding");
    let stroke = rig.item_id();
    let mark = rig.item_id();
    rig.edit(page, Edit::Add(bound_stroke(stroke, 1, binding)))
        .await;
    rig.edit(
        page,
        Edit::Add(
            MarkItem {
                frame: chosen,
                preset: penwise_core::item::FramePreset::Phone,
                status: MarkStatus::New,
                state: String::new(),
                strokes: vec![stroke],
            }
            .item(mark, 2),
        ),
    )
    .await;
    let bundled = json(&format!("{base}/variants/{group}")).await;
    let marks = bundled["marks"].as_array().expect("marks");
    assert_eq!(marks.len(), 1, "{bundled}");
    assert_eq!(marks[0]["frame"], chosen.to_hex());
    assert_eq!(marks[0]["kind"]["guess"], "circle");
    assert!(marks[0]["images"]["with_ink"].is_string(), "{bundled}");

    // Clearing removes the frames and leaves the ink as `done` in the page's log.
    let response = reqwest::Client::new()
        .delete(format!("{base}/variants/{group}"))
        .send()
        .await
        .expect("delete");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let cleared: Value = response.json().await.expect("clear json");
    assert_eq!(cleared["removed"], json!(3));
    assert_eq!(cleared["marks"], json!(1));
    let view = rig.view(page).await;
    let scene = view.replica.scene();
    for id in &ids {
        assert!(scene.get(*id).is_none(), "the variant frame is gone");
    }
    let kept = penwise_core::marks::mark_of(scene, mark).expect("the mark stays");
    assert_eq!(kept.status, MarkStatus::Done);
    let missing = reqwest::get(format!("{base}/variants/{group}"))
        .await
        .expect("get");
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn boards_are_titled_board_and_an_older_review_titled_board_still_counts() {
    let rig = Rig::new();
    let registry = rig.projects();
    let root = fixture_root();
    registry
        .link(LinkRequest {
            id: Some("prj_harness".to_owned()),
            name: Some("harness".to_owned()),
            root: root.clone(),
            folder: Some(root),
            origin: None,
        })
        .await
        .expect("links");
    let app = rig.app_with_projects(registry);
    let project = app
        .projects
        .get("prj_harness")
        .await
        .expect("get")
        .expect("project");

    // A v1 board: a page titled "<name> review" in the project's notebook.
    let notebook = project
        .notebook
        .as_deref()
        .expect("notebook")
        .parse()
        .expect("id");
    let old = PageId::from_parts(app.storage.now_ms(), 4242);
    let mut author = crate::api::Author::new(&app.storage);
    author
        .page(
            old,
            penwise_proto::PageChange::Create {
                notebook,
                title: "harness review".to_owned(),
                position: 1,
                kind: PageKind::Infinite,
                background: penwise_proto::Background::Blank,
            },
        )
        .await
        .expect("page");
    author.commit().await.expect("commit");
    assert_eq!(
        super::board::board_page(&app, &project)
            .await
            .expect("board"),
        Some(old),
        "the old title is still the board"
    );
    let (page, _) = super::frame_open_via_json(
        &app,
        "prj_harness",
        "/",
        penwise_core::item::FramePreset::Desktop,
    )
    .await
    .expect("open");
    assert_eq!(
        page, old,
        "frames go on the existing board, not a second one"
    );
}
