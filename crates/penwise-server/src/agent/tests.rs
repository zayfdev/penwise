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
use penwise_proto::{ItemRef, PageId};
use serde_json::{Value, json};

use crate::api::projects::routes as group;
use crate::frames::registry::LinkRequest;
use crate::testing::Rig;

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

    // The review page lives in the project's notebook, titled "<project> review".
    let catalog = rig.storage.catalog().await.expect("catalog");
    let review = catalog.page(page).expect("review page");
    assert_eq!(review.title, "harness review");
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

    // Side by side: the desktop frame starts to the right of the phone frame.
    let view = rig.view(page).await;
    let scene = view.replica.scene();
    let phone = scene.get(first_frame).expect("phone frame");
    let desktop = scene.get(second_frame).expect("desktop frame");
    assert_eq!(phone.transform.e, 0.0);
    assert_eq!(desktop.transform.e, 390.0 + 40.0, "a 40 px gap");
    assert_eq!(phone.transform.f, desktop.transform.f);

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
    assert_eq!(png_size(&png), (390, 844));

    let plain = reqwest::get(format!(
        "{base}/frames/{}/snapshot.png?ink=false",
        second_frame.to_hex()
    ))
    .await
    .expect("snapshot");
    assert_eq!(plain.status(), reqwest::StatusCode::OK);
    assert_eq!(png_size(&plain.bytes().await.expect("bytes")), (1440, 900));
}
