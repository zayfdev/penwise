//! Tests that need no browser: the failure cache, staleness, the hit map in memory, and the strip store.

use std::sync::atomic::Ordering;

use super::super::blobs::{self, BlobStore};
use super::super::model::{Hit, Still, StripMeta, hash_of, is_hash};
use super::super::scale;
use super::super::slots::FAILURE_TTL;
use super::super::triggers;
use super::*;

/// A board frame whose project is not linked: capturing it fails before any browser starts.
async fn orphan_frame(site: &Site) -> ItemId {
    let spec = penwise_core::item::FrameSpec::new(
        "prj_missing".to_owned(),
        "/".to_owned(),
        FramePreset::Desktop,
        None,
        None,
        0.0,
        0.0,
    )
    .expect("spec");
    let id = ItemRef::from_parts(1_800_000_000_000, 77).into();
    let mut author = crate::api::Author::new(&site.rig.storage);
    author
        .item(
            site.page,
            penwise_core::item::add_frame(&spec, id, Layer::Ink, 5),
        )
        .await
        .expect("author");
    author.commit().await.expect("commit");
    id
}

fn captures(site: &Site) -> u64 {
    site.app.stills.inner.captures.load(Ordering::SeqCst)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_capture_is_answered_from_memory_with_retry_after() {
    let site = site("sticky-header").await;
    let frame = orphan_frame(&site).await;

    let first = site.app.stills.refresh(frame).await.expect_err("fails");
    assert!(
        matches!(first, super::super::StillError::Agent(_)),
        "{first}"
    );
    // Five more asks at once: none opens another page.
    let more = futures_util::future::join_all((0..5).map(|_| site.app.stills.refresh(frame))).await;
    for result in more {
        match result.expect_err("still failing") {
            super::super::StillError::Unavailable { retry_after, .. } => {
                assert!((1..=FAILURE_TTL.as_secs()).contains(&retry_after));
            }
            other => panic!("expected the cached failure, got {other}"),
        }
    }
    assert_eq!(captures(&site), 1, "the error cache absorbed the rest");

    let response = reqwest::get(site.url(frame, "still")).await.expect("get");
    assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    let retry: u64 = response.headers()["retry-after"]
        .to_str()
        .expect("header")
        .parse()
        .expect("seconds");
    assert!(retry >= 1);
    assert_eq!(captures(&site), 1, "the poll did not retry either");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_trigger_clears_the_failure_and_leaves_an_unpolled_frame_stale() {
    let site = site("sticky-header").await;
    let frame = orphan_frame(&site).await;
    site.app.stills.refresh(frame).await.expect_err("fails");
    assert!(site.app.stills.inner.recent_failure(frame).is_some());

    // Something changed (a route fix, a reload): the failure is forgotten, but nobody polled, so no capture.
    let inner = Arc::clone(&site.app.stills.inner);
    inner.with_slot(frame, |slot| slot.polled = None);
    triggers::fire(Arc::clone(&inner), frame, Instant::now()).await;
    assert!(inner.recent_failure(frame).is_none());
    assert!(inner.is_stale(frame));
    assert_eq!(captures(&site), 1, "no eager capture for an unpolled frame");

    // A polled frame is shot by its trigger.
    inner.note_poll(frame);
    triggers::fire(Arc::clone(&inner), frame, Instant::now()).await;
    assert_eq!(captures(&site), 2);
}

fn hit(selector: &str, rect: [f32; 4]) -> Hit {
    Hit {
        key: format!("v1||{selector}"),
        selector: selector.to_owned(),
        rect,
    }
}

fn still_with(hits: Vec<Hit>) -> Still {
    Still {
        version: 7,
        project: "prj_sites".to_owned(),
        doc: [1440, 900],
        viewport: [1440, 900],
        strips: vec![StripMeta {
            y: 0,
            h: 900,
            hash: "0".repeat(32),
        }],
        rects: std::collections::BTreeMap::new(),
        state: String::new(),
        note: None,
        hits: Arc::new(hits),
    }
}

#[test]
fn the_first_hit_containing_the_point_wins() {
    let still = still_with(vec![
        hit("#over", [150.0, 150.0, 200.0, 200.0]),
        hit("#under", [100.0, 100.0, 200.0, 200.0]),
        hit("body", [0.0, 0.0, 1440.0, 900.0]),
    ]);
    assert_eq!(still.hit_at([200.0, 200.0])["selector"], "#over");
    assert_eq!(still.hit_at([110.0, 110.0])["selector"], "#under");
    assert_eq!(still.hit_at([900.0, 800.0])["selector"], "body");
    assert_eq!(
        still.hit_at([900.0, 800.0])["rect"],
        json!([0.0, 0.0, 1440.0, 900.0])
    );
    let none = still.hit_at([5000.0, 5000.0]);
    assert_eq!(none, json!({ "key": null, "selector": null, "rect": null }));
}

/// A route-level hit on a frame whose still is cached answers although the browser binary does not exist.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hit_is_answered_from_the_still_without_a_browser() {
    let mut site = site("sticky-header").await;
    let frame = site.frame(0.0, 0.0).await;
    let stills = super::super::Stills::new(
        site.rig.storage.clone(),
        Arc::new(crate::inspect::Inspect::with_chromium(
            "/nonexistent/chromium",
        )),
        Arc::clone(&site.app.projects),
        site.rig.dir.path().join("stills-offline"),
    );
    stills.inner.cache().put(
        frame,
        Arc::new(still_with(vec![hit(
            "#marker",
            [300.0, 3000.0, 200.0, 100.0],
        )])),
    );
    site.app.stills = Arc::new(stills);
    let base = site
        .rig
        .serve_app(group(Arc::new(String::new())), site.app.clone())
        .await;
    let url = |tail: &str| format!("{base}/frames/{}/{tail}", frame.to_hex());

    let answer: Value = reqwest::get(url("hit?x=400&y=3050"))
        .await
        .expect("hit")
        .json()
        .await
        .expect("json");
    assert_eq!(answer["selector"], "#marker");
    assert_eq!(answer["rect"], json!([300.0, 3000.0, 200.0, 100.0]));

    // Without a still the browser path is the fallback, and here it cannot start.
    let other = site.frame(0.0, 5000.0).await;
    let response = reqwest::get(format!("{base}/frames/{}/hit?x=1&y=1", other.to_hex()))
        .await
        .expect("hit");
    assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn the_blob_store_evicts_oldest_first_and_spills_to_a_pruned_directory() {
    let mut store = BlobStore::new(250);
    let blob = |n: usize| axum::body::Bytes::from(vec![7_u8; n]);
    assert!(store.put("a", blob(100)).is_empty());
    assert!(store.put("b", blob(100)).is_empty());
    assert!(store.get("a").is_some(), "touch a: b is now the oldest");
    let evicted = store.put("c", blob(100));
    assert_eq!(
        evicted
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        vec!["b"]
    );
    assert_eq!(store.bytes(), 200);
    // The newest blob stays even when it alone is over the budget.
    let evicted = store.put("d", blob(1000));
    assert_eq!(evicted.len(), 2);
    assert_eq!(store.bytes(), 1000);

    let dir = penwise_testkit::TempDir::new("blobs").expect("tempdir");
    let target = dir.path().join("spill");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        blobs::spill(target.clone(), evicted, 150).await;
        // 100 + 100 bytes written, pruned to 150: one file is left.
        let left = std::fs::read_dir(&target).expect("dir").count();
        assert_eq!(left, 1);
    });
}

#[test]
fn hashes_are_content_addresses() {
    let one = hash_of(b"png bytes");
    assert!(is_hash(&one));
    assert_eq!(one, hash_of(b"png bytes"));
    assert_ne!(one, hash_of(b"other bytes"));
    assert!(!is_hash("../etc/passwd"));
    assert!(!is_hash(&"A".repeat(32)));
}

#[test]
fn widths_snap_up_to_a_bucket() {
    assert_eq!(scale::bucket(1), 360);
    assert_eq!(scale::bucket(360), 360);
    assert_eq!(scale::bucket(361), 720);
    assert_eq!(scale::bucket(1440), 1440);
    assert_eq!(scale::bucket(5000), 1440);
    let mut source = tiny_skia::Pixmap::new(8, 4).expect("pixmap");
    source.fill(tiny_skia::Color::from_rgba8(200, 100, 50, 255));
    let small = scale::downscale(&source, 2).expect("scaled");
    assert_eq!((small.width(), small.height()), (2, 1));
    let pixel = small.pixel(1, 0).expect("pixel");
    assert_eq!((pixel.red(), pixel.green(), pixel.blue()), (200, 100, 50));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_poll_never_waits_for_the_capture_it_starts() {
    let site = site("sticky-header").await;
    let frame = orphan_frame(&site).await;
    let stills = &site.app.stills;

    // Cold: the answer is immediate and says when to ask again; the capture runs on its own.
    let first = stills.current(frame).expect_err("no still yet");
    assert!(
        matches!(first, super::super::StillError::Pending { retry_after: 2 }),
        "{first}"
    );
    let response = reqwest::get(site.url(frame, "still")).await.expect("get");
    assert_eq!(response.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.headers()["retry-after"], "2");

    // The background capture (which fails here: the project is not linked) ran once for all those polls, and
    // its failure is what the next poll hears.
    let started = Instant::now();
    loop {
        match stills.current(frame) {
            Err(super::super::StillError::Unavailable { .. }) => break,
            Err(super::super::StillError::Pending { .. }) => {}
            other => panic!("unexpected {other:?}"),
        }
        assert!(started.elapsed() < Duration::from_secs(90), "no result");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(captures(&site), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn growing_a_document_moves_the_lanes_below_it_down() {
    use super::super::lanes::{LANE_GAP, restack};
    let mut site = site("sticky-header").await;
    let mut lane = |route: &str, y: f32| {
        let spec = penwise_core::item::FrameSpec::new(
            "prj_sites".to_owned(),
            route.to_owned(),
            FramePreset::Desktop,
            None,
            None,
            0.0,
            y,
        )
        .expect("spec");
        let id: ItemId = site.rig.item_id();
        (id, penwise_core::item::add_frame(&spec, id, Layer::Ink, 1))
    };
    // Three lanes of 900 px boxes, 200 apart: tops at 0, 1100, 2200.
    let (a, add_a) = lane("/", 0.0);
    let (b, add_b) = lane("/b", 1100.0);
    let (c, add_c) = lane("/c", 2200.0);
    // B's history picture sits at its top edge.
    let picture = Item {
        id: site.rig.item_id(),
        layer: Layer::Ink,
        z: 2,
        transform: Affine::translate(-500.0, 1100.0),
        kind: ItemKind::Image(penwise_core::item::ImageItem {
            asset: [0; 32],
            w: 300.0,
            h: 400.0,
        }),
    };
    let picture_id = picture.id;
    for edit in [add_a, add_b, add_c, Edit::Add(picture)] {
        site.rig.edit(site.page, edit).await;
    }
    let view = site.rig.view(site.page).await;
    let scene = view.replica.scene();

    let tops = |edits: &[Edit]| -> std::collections::BTreeMap<ItemId, f32> {
        edits
            .iter()
            .filter_map(|edit| match edit {
                Edit::Update { id, patch } => patch.transform.map(|t| (*id, t.f)),
                _ => None,
            })
            .collect()
    };
    // A fits (900 <= 1100 - 200): nothing moves.
    assert!(restack(scene, a, 900).is_empty());
    // A grows to 3000 px: B moves under it (3000 + 200), with its picture, and C under B (B's box is 900 tall).
    let moved = tops(&restack(scene, a, 3000));
    assert_eq!(moved.get(&b), Some(&(3000.0 + LANE_GAP)));
    assert_eq!(moved.get(&picture_id), Some(&(3000.0 + LANE_GAP)));
    assert_eq!(moved.get(&c), Some(&(3000.0 + LANE_GAP + 900.0 + LANE_GAP)));
    assert!(!moved.contains_key(&a), "the grown lane stays");
    // B grows: only C (below it) moves; A (above) stays.
    let moved = tops(&restack(scene, b, 2000));
    assert_eq!(moved.keys().copied().collect::<Vec<_>>(), vec![c]);
    assert_eq!(moved.get(&c), Some(&(1100.0 + 2000.0 + LANE_GAP)));
    // Shrinking never pulls lanes back up.
    assert!(restack(scene, a, 100).is_empty());
}
