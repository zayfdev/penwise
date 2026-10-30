//! Stills, hits and document-space crops against real pages in headless Chromium (spec v2 §8 fixtures under
//! `fixtures/sites/`), plus the pure parts: the cache, the trigger rules, the strip arithmetic.
//!
//! Chromium is the feature: without `/usr/bin/chromium` (or `PENWISE_CHROMIUM`) the browser tests are skipped
//! with a message, like the other headless tests.

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
use std::time::{Duration, Instant};

use penwise_core::brush::BrushParams;
use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::item::{
    Affine, FrameExtent, FramePreset, Item, ItemId, ItemKind, Layer, StrokeItem, set_patch,
};
use penwise_core::marks::{Binding, BindingMode, MarkItem, MarkStatus};
use penwise_core::sample::Sample;
use penwise_proto::{ItemRef, PageId, PageKind};
use serde_json::{Value, json};
use tiny_skia::Pixmap;

use super::model::{INFINITE_NOTE, etag_matches};
use super::strips::{self, STRIP_HEIGHT};
use super::triggers::{Change, classify};

use crate::api::AppState;
use crate::api::projects::routes as group;
use crate::frames::registry::LinkRequest;
use crate::testing::Rig;

const RED: u32 = 0xFFFF_0000;

fn chromium() -> bool {
    std::env::var_os("PENWISE_CHROMIUM")
        .map_or_else(|| PathBuf::from("/usr/bin/chromium"), PathBuf::from)
        .is_file()
}

macro_rules! need_chromium {
    () => {
        if !chromium() {
            eprintln!("skipping: no chromium at /usr/bin/chromium (set PENWISE_CHROMIUM)");
            return;
        }
    };
}

mod pages;
mod service;

fn site_dir(case: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/sites")
        .join(case)
}

/// A rig with the `fixtures/sites/<case>` folder (or `folder`) linked as `prj_sites`, the project + still
/// routes served on a loopback port, and a board page.
struct Site {
    rig: Rig,
    app: AppState,
    base: String,
    page: PageId,
}

async fn site_at(folder: PathBuf) -> Site {
    site_with(folder, None).await
}

/// [`site_at`] with the still service's strip memory limited to `memory` bytes (to force spills to disk).
async fn site_with(folder: PathBuf, memory: Option<usize>) -> Site {
    let mut rig = Rig::new();
    let registry = rig.projects();
    registry
        .link(LinkRequest {
            id: Some("prj_sites".to_owned()),
            name: Some("sites".to_owned()),
            root: folder.clone(),
            folder: Some(folder),
            origin: None,
        })
        .await
        .expect("fixture project links");
    let mut app = rig.app_with_projects(registry);
    if let Some(memory) = memory {
        app.stills = Arc::new(super::Stills::with_memory(
            rig.storage.clone(),
            Arc::clone(&app.inspect),
            Arc::clone(&app.projects),
            rig.dir.path().join("stills"),
            memory,
        ));
    }
    let base = rig
        .serve_app(group(Arc::new(String::new())), app.clone())
        .await;
    let page = rig
        .page(PageKind::Infinite, penwise_proto::Background::Blank)
        .await;
    Site {
        rig,
        app,
        base,
        page,
    }
}

async fn site(case: &str) -> Site {
    site_at(site_dir(case)).await
}

impl Site {
    /// A desktop frame at `(x, y)` of the board page.
    async fn frame(&self, x: f32, y: f32) -> ItemId {
        let body = json!({
            "project": "prj_sites", "route": "/", "preset": "desktop", "x": x, "y": y
        });
        let response = reqwest::Client::new()
            .post(format!("{}/pages/{}/frames", self.base, self.page))
            .json(&body)
            .send()
            .await
            .expect("post");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let value: Value = response.json().await.expect("frame json");
        let text = value["id"].as_str().expect("frame id");
        ItemId::from_hex(text)
            .or_else(|| text.parse::<ItemRef>().ok().map(ItemId::from))
            .expect("frame id parses")
    }

    fn url(&self, frame: ItemId, tail: &str) -> String {
        format!("{}/frames/{}/{tail}", self.base, frame.to_hex())
    }

    /// `GET still`, polled the way a client does: a `503` with `Retry-After` means the capture runs in the
    /// background, so wait that long and ask again.
    async fn still(&self, frame: ItemId) -> (Value, reqwest::header::HeaderMap) {
        let started = Instant::now();
        loop {
            let response = reqwest::get(self.url(frame, "still")).await.expect("get");
            if response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE
                && response.headers().contains_key("retry-after")
                && started.elapsed() < Duration::from_secs(90)
            {
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
            assert_eq!(response.status(), reqwest::StatusCode::OK);
            let headers = response.headers().clone();
            return (response.json().await.expect("json"), headers);
        }
    }

    async fn strip(&self, still: &Value, index: usize) -> Pixmap {
        let url = still["strips"][index]["url"].as_str().expect("strip url");
        let response = reqwest::get(format!("{}{url}", self.base))
            .await
            .expect("strip");
        assert_eq!(response.status(), reqwest::StatusCode::OK, "{url}");
        Pixmap::decode_png(&response.bytes().await.expect("png")).expect("decodes")
    }

    async fn frame_item(&self, frame: ItemId) -> penwise_core::item::FrameItem {
        let view = self.rig.view(self.page).await;
        match &view.replica.scene().get(frame).expect("frame").kind {
            ItemKind::Frame(item) => item.clone(),
            other => panic!("not a frame: {other:?}"),
        }
    }
}

fn rgb(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 3] {
    let pixel = pixmap.pixel(x, y).expect("pixel in range");
    [pixel.red(), pixel.green(), pixel.blue()]
}

/// Polls `GET still` until its version differs from `known` (at most `limit`); returns the new JSON.
async fn changed_within(site: &Site, frame: ItemId, known: u64, limit: Duration) -> Option<Value> {
    let started = Instant::now();
    loop {
        let (still, _) = site.still(frame).await;
        if still["version"].as_u64().expect("version") != known {
            return Some(still);
        }
        if started.elapsed() > limit {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// [`changed_within`] 25 s, failing the test when nothing changes.
async fn next_version(site: &Site, frame: ItemId, known: u64) -> Value {
    changed_within(site, frame, known, Duration::from_secs(25))
        .await
        .unwrap_or_else(|| panic!("no new still within 25 s (version {known})"))
}

/* ------------------------------------------------------------------ stills */

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_5000px_page_stills_in_strips_with_an_etag_and_writes_its_height() {
    need_chromium!();
    let site = site("sticky-header").await;
    let frame = site.frame(10.0, 20.0).await;

    let (still, headers) = site.still(frame).await;
    assert_eq!(still["doc"], json!([1440, 5000]), "{still}");
    assert_eq!(still["viewport"], json!([1440, 900]));
    let strips = still["strips"].as_array().expect("strips");
    assert_eq!(strips.len(), 2, "{still}");
    assert_eq!(
        (strips[0]["y"].clone(), strips[0]["h"].clone()),
        (json!(0), json!(4096))
    );
    assert_eq!(
        (strips[1]["y"].clone(), strips[1]["h"].clone()),
        (json!(4096), json!(904))
    );
    let version = still["version"].as_u64().expect("version");
    let url = strips[0]["url"].as_str().expect("url");
    let hash = url
        .strip_prefix(&format!("/frames/{}/strip/", frame.to_hex()))
        .and_then(|rest| rest.strip_suffix(".png"))
        .expect("a content-addressed URL");
    assert!(super::model::is_hash(hash), "{url}");
    assert_eq!(still["layout"]["rects"], json!({}));
    assert_eq!(still["layout"]["state"], "");
    assert!(still.get("note").is_none(), "no note for a finite page");

    let first = site.strip(&still, 0).await;
    let second = site.strip(&still, 1).await;
    assert_eq!((first.width(), first.height()), (1440, 4096));
    assert_eq!((second.width(), second.height()), (1440, 904));
    // The sticky header is in the strip once, at the top; the last section is further down.
    assert_eq!(rgb(&first, 700, 10), [0x11, 0x22, 0x33]);

    // ETag = version; a matching If-None-Match is a 304 and an old one is a fresh 200.
    let etag = headers["etag"].to_str().expect("etag").to_owned();
    assert_eq!(etag, format!("\"{version}\""));
    let client = reqwest::Client::new();
    let same = client
        .get(site.url(frame, "still"))
        .header("If-None-Match", &etag)
        .send()
        .await
        .expect("conditional");
    assert_eq!(same.status(), reqwest::StatusCode::NOT_MODIFIED);
    assert_eq!(same.headers()["etag"].to_str().expect("etag"), etag);
    assert!(same.bytes().await.expect("body").is_empty());
    let weak = client
        .get(site.url(frame, "still"))
        .header("If-None-Match", format!("W/{etag}"))
        .send()
        .await
        .expect("weak");
    assert_eq!(weak.status(), reqwest::StatusCode::NOT_MODIFIED);
    let stale = client
        .get(site.url(frame, "still"))
        .header("If-None-Match", "\"1\"")
        .send()
        .await
        .expect("stale");
    assert_eq!(stale.status(), reqwest::StatusCode::OK);

    // Strips are content-addressed and immutable.
    let strip = reqwest::get(format!("{}{url}", site.base))
        .await
        .expect("strip");
    assert_eq!(
        strip.headers()["cache-control"],
        "private, max-age=31536000, immutable"
    );
    let unknown = reqwest::get(site.url(frame, &format!("strip/{}.png", "0".repeat(32))))
        .await
        .expect("unknown strip");
    assert_eq!(unknown.status(), reqwest::StatusCode::NOT_FOUND);
    let malformed = reqwest::get(site.url(frame, "strip/../x.png"))
        .await
        .expect("bad");
    assert_ne!(malformed.status(), reqwest::StatusCode::OK);

    // The measured height is written back as a server-device op.
    assert_eq!(site.frame_item(frame).await.doc_h, 5000);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_100vh_hero_is_900_px_tall_in_the_still() {
    need_chromium!();
    let site = site("hero-100vh").await;
    let frame = site.frame(0.0, 0.0).await;
    let (still, _) = site.still(frame).await;
    assert_eq!(still["doc"], json!([1440, 3000]), "{still}");
    let strip = site.strip(&still, 0).await;
    assert_eq!(
        rgb(&strip, 10, 899),
        [0xcc, 0, 0],
        "the hero reaches y = 899"
    );
    assert_eq!(
        rgb(&strip, 10, 900),
        [0xff, 0xff, 0xff],
        "and ends at 900: the viewport was never resized to the document"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reveal_on_scroll_sections_are_revealed_in_the_still() {
    need_chromium!();
    let site = site("reveal-on-scroll").await;
    let frame = site.frame(0.0, 0.0).await;
    let (still, _) = site.still(frame).await;
    assert_eq!(still["doc"], json!([1440, 4400]), "{still}");
    let strip = site.strip(&still, 0).await;
    let green = [0, 0xaa, 0];
    assert_eq!(rgb(&strip, 100, 1400), green, "first section");
    assert_eq!(
        rgb(&strip, 100, 3000),
        green,
        "second section, far below the fold"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lazy_images_are_loaded_in_the_still_and_the_document_has_grown() {
    need_chromium!();
    let site = site("lazy-images").await;
    let frame = site.frame(0.0, 0.0).await;
    let (still, _) = site.still(frame).await;
    // 200 + 1800 + 600 + 1800 + 600: without the pre-roll both images are zero high and this is 3800.
    assert_eq!(still["doc"], json!([1440, 5000]), "{still}");
    let first = site.strip(&still, 0).await;
    let second = site.strip(&still, 1).await;
    let blue = [0x20, 0x60, 0xe0];
    assert_eq!(rgb(&first, 100, 2300), blue, "first image");
    assert_eq!(rgb(&second, 100, 4700 - 4096), blue, "second image");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_infinite_feed_flips_the_frame_to_viewport_with_a_note() {
    need_chromium!();
    let site = site("infinite-feed").await;
    let frame = site.frame(0.0, 0.0).await;
    assert_eq!(site.frame_item(frame).await.extent, FrameExtent::Full);

    let (still, _) = site.still(frame).await;
    assert_eq!(still["note"], INFINITE_NOTE, "{still}");
    assert_eq!(still["doc"][1], 30_000, "the document is capped: {still}");
    assert_eq!(
        still["strips"].as_array().expect("strips").len(),
        1,
        "a viewport frame shows its first screen"
    );
    let item = site.frame_item(frame).await;
    assert_eq!(item.extent, FrameExtent::Viewport, "set by the server");
    assert_eq!(item.doc_h, 0, "an endless document has no height to hint");
}

/* ------------------------------------------------------------------ triggers */

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_patch_change_makes_a_new_still_and_an_unchanged_one_keeps_its_version() {
    need_chromium!();
    let site = site("sticky-header").await;
    site.app.stills.start();
    let frame = site.frame(0.0, 0.0).await;
    let (first, _) = site.still(frame).await;
    let version = first["version"].as_u64().expect("version");

    // Shooting an identical page again keeps the version (deterministic: no waiting for a trigger).
    let again = site.app.stills.refresh(frame).await.expect("refresh");
    assert_eq!(
        again.version, version,
        "an unchanged page keeps its version"
    );

    // Written the way the agent's ghost is: an op from the server device, stamped after every earlier op (a
    // test device's clock would lose the field's last-writer-wins race against the frame's creation).
    let view = site.rig.view(site.page).await;
    let patch = json!({ "css": "main { background: #f00; }", "dom": [] }).to_string();
    let mut author = crate::api::Author::new(&site.rig.storage);
    for edit in set_patch(view.replica.scene(), frame, Some(patch)).expect("patch edit") {
        author.item(site.page, edit).await.expect("author");
    }
    author.commit().await.expect("commit");
    // The frame is being polled, so the trigger re-shoots it eagerly.
    let second = next_version(&site, frame, version).await;
    assert!(second["version"].as_u64().expect("version") > version);
    let strip = site.strip(&second, 0).await;
    assert_eq!(rgb(&strip, 5, 2000), [0xff, 0, 0], "the patched page");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_static_reload_makes_a_new_still() {
    need_chromium!();
    let copy = penwise_testkit::TempDir::new("still-reload").expect("tempdir");
    let page = std::fs::read_to_string(site_dir("sticky-header").join("index.html")).expect("page");
    std::fs::write(copy.path().join("index.html"), &page).expect("copy");
    let site = site_at(copy.path().to_path_buf()).await;
    site.app.stills.start();
    let frame = site.frame(0.0, 0.0).await;
    let (first, _) = site.still(frame).await;
    let version = first["version"].as_u64().expect("version");

    // The edit makes main red. Under load the folder watcher may not be armed yet and miss the first write, so
    // the edit is repeated (same colour, a changing comment) until a reload is seen.
    let mut second = None;
    for attempt in 0..5 {
        std::fs::write(
            copy.path().join("index.html"),
            page.replace("main { height", "main { background: #f00; height")
                + &format!("<!-- edit {attempt} -->"),
        )
        .expect("edit");
        second = changed_within(&site, frame, version, Duration::from_secs(15)).await;
        if second.is_some() {
            break;
        }
    }
    let second = second.expect("a new still after the edit");
    let strip = site.strip(&second, 0).await;
    assert_eq!(rgb(&strip, 5, 2000), [0xff, 0, 0], "red after the reload");
}

/* ------------------------------------------------------------------ document coordinates */

/// A closed loop of red ink around document point `(cx, cy)`, in area-binding (document) coordinates.
fn loop_item(id: ItemId, z: i64, binding: Binding, centre: [f32; 2]) -> Item {
    let samples = (0..=16)
        .map(|step| {
            #[allow(clippy::cast_precision_loss, reason = "16 steps")]
            let angle = step as f32 / 16.0 * std::f32::consts::TAU;
            Sample {
                x: centre[0] + 30.0 * angle.cos(),
                y: centre[1] + 26.0 * angle.sin(),
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: step * 8_333,
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
            params: BrushParams::for_preset(0, RED, 1.0),
            seed: 7,
            samples,
            trim: None,
            binding: Some(binding),
        }),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mark_at_document_y_3000_is_cropped_and_resolved_there() {
    need_chromium!();
    let mut site = site("sticky-header").await;
    let frame = site.frame(10.0, 20.0).await;
    let binding = Binding::new(
        frame,
        BindingMode::Area,
        FramePreset::Desktop,
        None,
        String::new(),
        [10.0, 20.0],
    )
    .expect("binding");
    let (stroke, mark) = (site.rig.item_id(), site.rig.item_id());
    site.rig
        .edit(
            site.page,
            Edit::Add(loop_item(stroke, 1, binding, [400.0, 3050.0])),
        )
        .await;
    site.rig
        .edit(
            site.page,
            Edit::Add(
                MarkItem {
                    frame,
                    preset: FramePreset::Desktop,
                    status: MarkStatus::New,
                    state: String::new(),
                    strokes: vec![stroke],
                }
                .item(mark, 2),
            ),
        )
        .await;

    let body: Value = reqwest::get(format!(
        "{}/marks?project=prj_sites&frame={}&agent=still-test",
        site.base,
        frame.to_hex()
    ))
    .await
    .expect("marks")
    .json()
    .await
    .expect("json");
    let marks = body["marks"].as_array().expect("marks");
    assert_eq!(marks.len(), 1, "{body}");
    let bundle = &marks[0];
    // The area mark's centre is below the first screen: the element comes from scrolling it into view.
    assert_eq!(bundle["element"]["selector"], "#marker", "{bundle}");
    let rect = bundle["rect"].as_array().expect("rect");
    let top = rect[1].as_f64().expect("y");
    assert!(
        (3000.0..3100.0).contains(&(top - 20.0 + 26.0)),
        "page rect is frame position + document position: {rect:?}"
    );

    let fetch = |kind: &str| {
        let url = bundle["images"][kind].as_str().expect(kind).to_owned();
        let base = site.base.clone();
        async move {
            let bytes = reqwest::get(format!("{base}{url}"))
                .await
                .expect("image")
                .bytes()
                .await
                .expect("bytes");
            Pixmap::decode_png(&bytes).expect("decodes")
        }
    };
    let plain = fetch("without_ink").await;
    let centre = (plain.width() / 2, plain.height() / 2);
    assert_eq!(
        rgb(&plain, centre.0, centre.1),
        [0xff, 0xaa, 0x00],
        "the element's orange"
    );
    let context = fetch("context").await;
    assert_eq!((context.width(), context.height()), (1440, 900));
    // The mark sits mid-height of the context region, which starts at 3050 - 450 = 2600.
    assert_eq!(rgb(&context, 320, 420), [0xff, 0xaa, 0x00]);
    assert_eq!(
        rgb(&context, 100, 100),
        [0xee, 0xee, 0xff],
        "a section further up"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hit_at_document_y_3000_resolves_the_element_there() {
    need_chromium!();
    let site = site("sticky-header").await;
    let frame = site.frame(0.0, 0.0).await;
    let hit: Value = reqwest::get(site.url(frame, "hit?x=400&y=3050"))
        .await
        .expect("hit")
        .json()
        .await
        .expect("json");
    assert_eq!(hit["selector"], "#marker", "{hit}");
    assert!(
        hit["key"].as_str().expect("key").starts_with("v1|"),
        "{hit}"
    );
    assert_eq!(
        hit["rect"],
        json!([300.0, 3000.0, 200.0, 100.0]),
        "document coordinates"
    );

    let brand: Value = reqwest::get(site.url(frame, "hit?x=40&y=40"))
        .await
        .expect("hit")
        .json()
        .await
        .expect("json");
    assert_eq!(brand["selector"], "#brand", "{brand}");

    let nothing: Value = reqwest::get(site.url(frame, "hit?x=40&y=9000"))
        .await
        .expect("hit")
        .json()
        .await
        .expect("json");
    assert_eq!(nothing["key"], Value::Null, "{nothing}");
    assert_eq!(nothing["rect"], Value::Null);

    let bad = reqwest::get(site.url(frame, "hit?x=NaN&y=1"))
        .await
        .expect("bad");
    assert_eq!(bad.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_snapshot_is_the_whole_full_length_box() {
    need_chromium!();
    let site = site("sticky-header").await;
    let frame = site.frame(0.0, 0.0).await;
    let png = reqwest::get(site.url(frame, "snapshot.png?ink=false"))
        .await
        .expect("snapshot")
        .bytes()
        .await
        .expect("bytes");
    let pixmap = Pixmap::decode_png(&png).expect("decodes");
    assert_eq!((pixmap.width(), pixmap.height()), (1440, 5000));
    // Stitched across the 4096 px strip seam without a gap.
    assert_eq!(rgb(&pixmap, 100, 4095), rgb(&pixmap, 100, 4096));
}

/* ------------------------------------------------------------------ pure parts */

fn id(n: u128) -> ItemId {
    ItemRef::from_parts(1, n).into()
}

#[test]
fn if_none_match_accepts_lists_weak_validators_and_star() {
    assert!(etag_matches("\"7\"", "\"7\""));
    assert!(etag_matches("W/\"7\"", "\"7\""));
    assert!(etag_matches("\"3\", \"7\"", "\"7\""));
    assert!(etag_matches("*", "\"7\""));
    assert!(!etag_matches("\"8\"", "\"7\""));
}

#[test]
fn strips_cover_the_document_in_pieces_of_at_most_4096() {
    assert_eq!(strips::spans(1440, 900), vec![(0, 900)]);
    assert_eq!(strips::spans(1440, 5000), vec![(0, 4096), (4096, 904)]);
    assert_eq!(strips::spans(1440, 8192), vec![(0, 4096), (4096, 4096)]);
    assert!(
        strips::spans(1440, 30_000)
            .iter()
            .all(|(_, h)| *h <= STRIP_HEIGHT)
    );
    assert_eq!(strips::spans(1440, 0), Vec::<(u32, u32)>::new());
}

#[test]
fn wide_strips_are_capped_at_six_megapixels() {
    assert_eq!(strips::strip_height(1440), 4096);
    assert_eq!(strips::strip_height(3000), 2000);
    assert_eq!(strips::strip_height(8000), 750);
    assert_eq!(
        strips::spans(3000, 5000),
        vec![(0, 2000), (2000, 2000), (4000, 1000)]
    );
    assert!(
        strips::spans(3000, 5000)
            .iter()
            .all(|(_, h)| u64::from(*h) * 3000 <= 6_000_000)
    );
}

#[test]
fn only_content_changes_of_a_frame_retrigger_a_still() {
    use penwise_core::item::FrameSize;
    use penwise_proto::{AnyOp, DeviceId, Hlc, Op, OpId};

    let frame = id(9);
    let device = DeviceId::from_parts(1, 1);
    let op = |edit: Edit| {
        AnyOp::Item(Box::new(Op {
            op_id: OpId::from_parts(1, 1),
            device,
            hlc: Hlc::zero(device),
            page: PageId::from_parts(1, 1),
            edit,
        }))
    };
    let update = |patch: ItemPatch| op(Edit::Update { id: frame, patch });
    let no_flip = |_: ItemId| false;

    let added = Item {
        id: frame,
        layer: Layer::Ink,
        z: 1,
        transform: Affine::IDENTITY,
        kind: ItemKind::Frame(penwise_core::item::FrameItem::new(
            "p".into(),
            "/".into(),
            FramePreset::Desktop,
            1440,
            900,
        )),
    };
    assert_eq!(
        classify(&op(Edit::Add(added)), no_flip),
        Change::Restill(frame)
    );
    let route = ItemPatch {
        frame_route: Some("/x".into()),
        ..ItemPatch::default()
    };
    assert_eq!(classify(&update(route), no_flip), Change::Restill(frame));
    let preset = ItemPatch {
        frame_preset: FrameSize::resolve(FramePreset::Phone, None, None).ok(),
        ..ItemPatch::default()
    };
    assert_eq!(classify(&update(preset), no_flip), Change::Restill(frame));
    let patch = ItemPatch {
        frame_patch: Some(None),
        ..ItemPatch::default()
    };
    assert_eq!(classify(&update(patch), no_flip), Change::Restill(frame));
    let extent = ItemPatch {
        frame_extent: Some(FrameExtent::Viewport),
        ..ItemPatch::default()
    };
    assert_eq!(
        classify(&update(extent.clone()), no_flip),
        Change::Restill(frame)
    );
    assert_eq!(
        classify(&update(extent), |flipped| flipped == frame),
        Change::Ignore,
        "the server's own switch to viewport is not a trigger"
    );
    let measured = ItemPatch {
        frame_doc_h: Some(5000),
        ..ItemPatch::default()
    };
    assert_eq!(classify(&update(measured), no_flip), Change::Ignore);
    let moved = ItemPatch {
        transform: Some(Affine::translate(5.0, 5.0)),
        ..ItemPatch::default()
    };
    assert_eq!(classify(&update(moved), no_flip), Change::Ignore);
    assert_eq!(
        classify(&op(Edit::Delete(frame)), no_flip),
        Change::Gone(frame)
    );

    // A stroke bound to an element makes the frame watch it; area strokes and plain ones change nothing.
    let binding = |mode, key: Option<&str>| {
        Binding::new(
            frame,
            mode,
            FramePreset::Desktop,
            key.map(str::to_owned),
            String::new(),
            [0.0, 0.0],
        )
        .expect("binding")
    };
    let bound = loop_item(
        id(3),
        1,
        binding(BindingMode::Bound, Some("v1||h1#brand:nth-of-type(1)")),
        [5.0, 5.0],
    );
    assert_eq!(
        classify(&op(Edit::Add(bound)), no_flip),
        Change::Watch(frame, "v1||h1#brand:nth-of-type(1)".to_owned())
    );
    let area = loop_item(id(4), 1, binding(BindingMode::Area, None), [5.0, 5.0]);
    assert_eq!(classify(&op(Edit::Add(area)), no_flip), Change::Ignore);
}
