//! Browser tests: the fixtures of spec §8, the hit map, scaled and noisy strips, coalescing, watched keys and the
//! strip store's spill to disk.

use std::sync::atomic::Ordering;

use super::*;

fn captures(site: &Site) -> u64 {
    site.app.stills.inner.captures.load(Ordering::SeqCst)
}

/// Polls `check` every 100 ms for up to 20 s.
async fn until<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let started = Instant::now();
    while !check().await {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn hit(site: &Site, frame: ItemId, x: f32, y: f32) -> Value {
    reqwest::get(site.url(frame, &format!("hit?x={x}&y={y}")))
        .await
        .expect("hit")
        .json()
        .await
        .expect("json")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_hit_map_orders_by_paint_order_and_agrees_with_the_browser() {
    need_chromium!();
    let site = site("stacking").await;
    let mapped = site.frame(0.0, 0.0).await;
    let direct = site.frame(0.0, 2000.0).await;
    site.still(mapped).await;
    // `direct` has no still: its hit opens a page and asks the bridge.
    for (x, y, want) in [
        (200.0, 200.0, "#over"),
        (120.0, 120.0, "#under"),
        // Within the first screen, where the browser path (which scrolls to a point) and the still agree: a fixed
        // bar is at its scroll-0 place in the still, but follows the scroll when the browser looks further down.
        (800.0, 300.0, "#flow"),
        (600.0, 430.0, "#bar"),
    ] {
        let from_map = hit(&site, mapped, x, y).await;
        assert_eq!(from_map["selector"], want, "map at ({x}, {y}): {from_map}");
        let from_browser = hit(&site, direct, x, y).await;
        assert_eq!(from_browser["selector"], want, "browser at ({x}, {y})");
        assert_eq!(from_map["rect"], from_browser["rect"]);
        assert_eq!(from_map["key"], from_browser["key"]);
    }
    let nothing = hit(&site, mapped, 100.0, 9000.0).await;
    assert_eq!(nothing["key"], Value::Null, "{nothing}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn strips_can_be_fetched_downscaled_to_a_bucket() {
    need_chromium!();
    let site = site("sticky-header").await;
    let frame = site.frame(0.0, 0.0).await;
    let (still, _) = site.still(frame).await;
    let url = still["strips"][0]["url"].as_str().expect("url").to_owned();
    let size = |query: &str| {
        let url = format!("{}{url}{query}", site.base);
        async move {
            let response = reqwest::get(url).await.expect("strip");
            assert_eq!(response.status(), reqwest::StatusCode::OK);
            assert_eq!(
                response.headers()["cache-control"],
                "private, max-age=31536000, immutable"
            );
            let bytes = response.bytes().await.expect("bytes");
            let png = Pixmap::decode_png(&bytes).expect("decodes");
            (png.width(), png.height(), bytes)
        }
    };
    let (w, h, small) = size("?w=360").await;
    assert_eq!((w, h), (360, 1024), "a quarter of 1440 x 4096");
    // The header colour survives averaging; the bytes are served again unchanged from the cache.
    let decoded = Pixmap::decode_png(&small).expect("decodes");
    assert_eq!(rgb(&decoded, 175, 5), [0x11, 0x22, 0x33]);
    let (_, _, again) = size("?w=360").await;
    assert_eq!(small, again);
    let (w, h, _) = size("?w=500").await;
    assert_eq!((w, h), (720, 2048), "500 snaps up to the 720 bucket");
    let (w, h, native) = size("?w=2000").await;
    assert_eq!((w, h), (1440, 4096), "never upscaled");
    let (_, _, plain) = size("").await;
    assert_eq!(native, plain);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_strip_too_big_for_the_library_default_does_not_drop_the_shared_connection() {
    need_chromium!();
    let site = site("noisy-image").await;
    let frame = site.frame(0.0, 0.0).await;
    let (still, _) = site.still(frame).await;
    let url = still["strips"][0]["url"].as_str().expect("url");
    let png = reqwest::get(format!("{}{url}", site.base))
        .await
        .expect("strip")
        .bytes()
        .await
        .expect("bytes");
    assert!(
        png.len() > 16 * 1024 * 1024,
        "the noisy strip is bigger than the default 16 MiB frame limit: {}",
        png.len()
    );
    // The browser connection every page shares still works.
    let other = site.frame(0.0, 6000.0).await;
    let (second, _) = site.still(other).await;
    assert_eq!(second["strips"][0]["h"], 4096);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn animations_do_not_make_an_unchanged_page_look_changed() {
    need_chromium!();
    let site = site("animated").await;
    let frame = site.frame(0.0, 0.0).await;
    let (first, _) = site.still(frame).await;
    let mut hashes = Vec::new();
    for _ in 0..3 {
        let again = site.app.stills.refresh(frame).await.expect("refresh");
        assert_eq!(
            again.version,
            first["version"].as_u64().expect("version"),
            "infinite animations are paused, so the picture is the same"
        );
        hashes.push(again.strips[0].hash.clone());
        tokio::time::sleep(Duration::from_millis(230)).await;
    }
    assert!(hashes.windows(2).all(|pair| pair[0] == pair[1]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_inner_scroll_box_does_not_stretch_the_document() {
    need_chromium!();
    let site = site("inner-scroll").await;
    let frame = site.frame(0.0, 0.0).await;
    let (still, _) = site.still(frame).await;
    assert_eq!(still["doc"], json!([1440, 1500]), "{still}");
    // A point inside the box resolves to what is on screen there (its first row), not to the page below it.
    let inside = hit(&site, frame, 40.0, 120.0).await;
    assert_eq!(
        inside["selector"]
            .as_str()
            .map(|s| s.contains("nth-of-type")),
        Some(true),
        "{inside}"
    );
    assert_ne!(inside["selector"], "#main");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smooth_scrolling_pages_are_rolled_through_and_revealed() {
    need_chromium!();
    let site = site("smooth-scroll").await;
    let frame = site.frame(0.0, 0.0).await;
    let (still, _) = site.still(frame).await;
    assert_eq!(still["doc"], json!([1440, 3400]), "{still}");
    let strip = site.strip(&still, 0).await;
    assert_eq!(
        rgb(&strip, 100, 1700),
        [0, 0xaa, 0],
        "revealed, at its final position"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_requests_and_triggers_cost_at_most_one_more_capture() {
    need_chromium!();
    let site = site("sticky-header").await;
    site.app.stills.start();
    let frame = site.frame(0.0, 0.0).await;
    let stills = &site.app.stills;
    let results = futures_util::future::join_all((0..6).map(|_| stills.refresh(frame))).await;
    let versions: Vec<u64> = results
        .iter()
        .map(|r| r.as_ref().expect("still").version)
        .collect();
    assert!(
        versions.windows(2).all(|pair| pair[0] == pair[1]),
        "{versions:?}"
    );
    // One running + one pending at most, however many asked at once.
    assert!(captures(&site) <= 2, "captures: {}", captures(&site));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_frame_is_shot_once_not_by_its_first_poll_and_again_by_its_trigger() {
    need_chromium!();
    let site = site("sticky-header").await;
    site.app.stills.start();
    let frame = site.frame(0.0, 0.0).await;
    site.still(frame).await;
    assert_eq!(captures(&site), 1);
    // The creation trigger fires 500 ms after the op; give it well past that, then count.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    site.still(frame).await;
    assert_eq!(
        captures(&site),
        1,
        "the creation trigger found a newer capture"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_frame_nobody_polls_is_marked_stale_and_shot_on_its_next_poll() {
    need_chromium!();
    let site = site("sticky-header").await;
    site.app.stills.start();
    let frame = site.frame(0.0, 0.0).await;
    let (first, _) = site.still(frame).await;
    let version = first["version"].as_u64().expect("version");
    let before = captures(&site);

    // The page changes while nobody looks: the patch fires its trigger, which only marks the frame stale.
    let inner = &site.app.stills.inner;
    inner.with_slot(frame, |slot| {
        slot.polled = None;
    });
    let view = site.rig.view(site.page).await;
    let patch = json!({ "css": "main { background: #f00; }", "dom": [] }).to_string();
    let mut author = crate::api::Author::new(&site.rig.storage);
    for edit in set_patch(view.replica.scene(), frame, Some(patch)).expect("patch edit") {
        author.item(site.page, edit).await.expect("author");
    }
    author.commit().await.expect("commit");
    until("the frame to be marked stale", || async {
        inner.with_slot(frame, |slot| slot.stale)
    })
    .await;
    assert_eq!(
        captures(&site),
        before,
        "nobody was looking: no eager capture"
    );

    // The next poll captures before it answers.
    let (second, _) = site.still(frame).await;
    assert!(second["version"].as_u64().expect("version") > version);
    assert_eq!(captures(&site), before + 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binding_a_mark_to_a_new_element_re_shoots_so_the_still_carries_its_rect() {
    need_chromium!();
    let mut site = site("sticky-header").await;
    site.app.stills.start();
    let frame = site.frame(0.0, 0.0).await;
    let (first, _) = site.still(frame).await;
    assert_eq!(first["layout"]["rects"], json!({}));

    let key = "v1||h1#brand:nth-of-type(1)";
    let binding = Binding::new(
        frame,
        BindingMode::Bound,
        FramePreset::Desktop,
        Some(key.to_owned()),
        String::new(),
        [0.0, 0.0],
    )
    .expect("binding");
    let stroke = site.rig.item_id();
    site.rig
        .edit(
            site.page,
            Edit::Add(loop_item(stroke, 1, binding, [10.0, 10.0])),
        )
        .await;
    let version = first["version"].as_u64().expect("version");
    let second = next_version(&site, frame, version).await;
    let rect = &second["layout"]["rects"][key];
    assert!(rect.is_array(), "the element's document rect: {second}");
    assert!(
        rect[1].as_f64().is_some_and(|y| (0.0..80.0).contains(&y)),
        "the h1 sits inside the 80 px header: {rect}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn evicted_strips_come_back_from_disk_and_the_version_survives_eviction() {
    need_chromium!();
    // A budget of one byte: every strip but the newest is spilled the moment the next one arrives.
    let site = site_with(site_dir("sticky-header"), Some(1)).await;
    let frame = site.frame(0.0, 0.0).await;
    let (first, _) = site.still(frame).await;
    let version = first["version"].as_u64().expect("version");
    let spill = site.rig.dir.path().join("stills");
    until("the first strip to reach the disk", || async {
        std::fs::read_dir(&spill).is_ok_and(|mut dir| dir.next().is_some())
    })
    .await;
    // Both strips are still served (one from memory, one read back from disk).
    assert_eq!(site.strip(&first, 0).await.height(), 4096);
    assert_eq!(site.strip(&first, 1).await.height(), 904);
    // Recapturing the identical page does not recapture-by-necessity or change the version.
    let again = site.app.stills.refresh(frame).await.expect("refresh");
    assert_eq!(again.version, version);
}

/// A dev server that rebuilds the moment a page connects to its HMR socket (as Next does while it compiles a
/// route on first load), and a page that connects.
async fn rebuilding_dev_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    use axum::extract::ws::{Message, WebSocketUpgrade};
    use axum::response::{Html, IntoResponse};
    use axum::routing::get;
    let app = axum::Router::new()
        .route(
            "/",
            get(|| async {
                Html(
                    "<!doctype html><title>dev</title><body style=\"margin:0\"><script>\
                     new WebSocket('ws://' + location.host + '/_next/webpack-hmr');</script></body>",
                )
            }),
        )
        .route(
            "/_next/webpack-hmr",
            get(|upgrade: WebSocketUpgrade| async move {
                upgrade
                    .on_upgrade(|mut socket| async move {
                        let _ = socket
                            .send(Message::Text("{\"action\":\"built\"}".into()))
                            .await;
                        while socket.recv().await.is_some() {}
                    })
                    .into_response()
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    (
        addr,
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        }),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_capture_pages_hmr_socket_never_triggers_the_next_capture() {
    need_chromium!();
    let (dev, dev_task) = rebuilding_dev_server().await;
    let mut rig = Rig::new();
    let registry = rig.projects();
    let root = site_dir("sticky-header");
    registry
        .link(LinkRequest {
            id: Some("prj_dev".to_owned()),
            name: Some("dev".to_owned()),
            root,
            folder: None,
            origin: Some(format!("http://{dev}")),
        })
        .await
        .expect("link");
    let mut heard = registry.subscribe_changes();
    let app = rig.app_with_projects(Arc::clone(&registry));
    let page = rig
        .page(PageKind::Infinite, penwise_proto::Background::Blank)
        .await;
    let spec = penwise_core::item::FrameSpec::new(
        "prj_dev".to_owned(),
        "/".to_owned(),
        FramePreset::Desktop,
        None,
        None,
        0.0,
        0.0,
    )
    .expect("spec");
    let frame: ItemId = ItemRef::from_parts(1_800_000_000_000, 5).into();
    let mut author = crate::api::Author::new(&rig.storage);
    author
        .item(
            page,
            penwise_core::item::add_frame(&spec, frame, Layer::Ink, 5),
        )
        .await
        .expect("author");
    author.commit().await.expect("commit");

    // The capture page connects to the HMR socket, which announces a rebuild to it: nobody is told.
    app.stills.refresh(frame).await.expect("still");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        heard.try_recv().is_err(),
        "a capture's socket is not an HMR listener"
    );

    // Control: the same page opened as a person's page is an HMR listener.
    let project = registry
        .get("prj_dev")
        .await
        .expect("get")
        .expect("project");
    let inspect = crate::inspect::Inspect::new();
    let _page = inspect
        .open(&registry.url(&project), 1440, 900)
        .await
        .expect("open");
    let project_id = tokio::time::timeout(Duration::from_secs(5), heard.recv())
        .await
        .expect("the control page's socket fires")
        .expect("feed");
    assert_eq!(project_id, "prj_dev");
    dev_task.abort();
}
