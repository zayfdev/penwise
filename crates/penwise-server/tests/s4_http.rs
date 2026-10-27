//! Scenarios that need the S4 HTTP API (annotate, PNG render, restore). Written against the planned routes
//! (`docs/rewrite-design.md` §1.5, §3.5; JSON shapes unchanged for MCP) and ignored until S4 lands.
//!
//! The harness does not start an HTTP router yet (`serve()` mounts `/sync` and `/health` only): S4 adds the routes
//! to `serve()`, after which these are enabled and `base()` below is the only thing to adjust.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::similar_names,
    clippy::items_after_statements
)]

mod common;

use std::time::Duration;

use common::{TestServer, converge, line};
use penwise_core::item::{Layer, LayerSet};
use penwise_proto::PageId;

fn base(server: &TestServer) -> String {
    format!("http://{}", server.addr)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

fn auth() -> (&'static str, &'static str) {
    ("X-Penwise-Token", common::TOKEN)
}

/// H-5: negative/zero/huge seeds through `/sync` render pixel-equal to an in-process `Scene` render of the same ops.
#[tokio::test]
#[ignore = "needs S4: GET /page.png on the op-log server"]
async fn h05_seed_edge_cases_render_pixel_equal_to_the_in_process_scene() {
    let server = TestServer::start().await;
    let mut a = server.tablet(1);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "P");
    for (i, seed) in [
        0_u64,
        1,
        u64::MAX,
        i64::MIN.cast_unsigned(),
        (-2_520_638_596_094_309_648_i64).cast_unsigned(),
    ]
    .into_iter()
    .enumerate()
    {
        a.draw_with(
            page,
            Layer::Ink,
            seed,
            &line(20.0, 60.0 + i as f32 * 70.0, 40),
        );
    }
    converge(&server, &mut [&mut a]).await;

    let url = format!("{}/page.png?page={page}&scale=1", base(&server));
    let (k, v) = auth();
    let png = client()
        .get(url)
        .header(k, v)
        .send()
        .await
        .expect("get")
        .bytes()
        .await
        .expect("body");
    let served = tiny_skia::Pixmap::decode_png(&png).expect("png");
    let scene = a.replica.page(page).expect("page").scene();
    let bounds = penwise_core::Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 1000.0,
        max_y: 1000.0,
    };
    let local = penwise_core::raster::render_scene(
        scene,
        LayerSet::ALL,
        &bounds,
        1.0,
        &std::collections::BTreeMap::new(),
    )
    .expect("render");
    assert_eq!(
        served.data(),
        local.data(),
        "server PNG == in-process render of the same ops"
    );
}

/// H-6: annotate ×N across a server restart, then clear; a promoted mark survives the clear.
#[tokio::test]
#[ignore = "needs S4: POST /annotate and /annotate/clear on ops"]
async fn h06_annotate_across_restart_then_clear_removes_exactly_agent_items() {
    let mut server = TestServer::start().await;
    let mut a = server.tablet(1);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "P");
    let ink = a.draw(page, &line(10.0, 10.0, 10));
    converge(&server, &mut [&mut a]).await;
    let (k, v) = auth();
    let body = serde_json::json!({"page": page.to_string(), "strokes": [{"points": [[10, 10], [60, 60]], "color": "#ff0000"}]});
    for _ in 0..3 {
        let r = client()
            .post(format!("{}/annotate", base(&server)))
            .header(k, v)
            .json(&body)
            .send()
            .await
            .expect("annotate");
        assert!(r.status().is_success());
    }
    server.restart().await;
    for _ in 0..3 {
        let r = client()
            .post(format!("{}/annotate", base(&server)))
            .header(k, v)
            .json(&body)
            .send()
            .await
            .expect("annotate");
        assert!(r.status().is_success());
    }
    a.online().await.expect("online");
    let agent: Vec<_> = a
        .live_items(page)
        .into_iter()
        .filter(|i| *i != ink)
        .collect();
    assert_eq!(agent.len(), 6, "no id or seed collision across a restart");
    a.set_layer(page, agent[0], Layer::Ink);
    a.settle(Duration::from_secs(10)).await.expect("settle");
    let r = client()
        .post(format!("{}/annotate/clear", base(&server)))
        .header(k, v)
        .json(&serde_json::json!({"page": page.to_string()}))
        .send()
        .await
        .expect("clear");
    assert!(r.status().is_success());
    a.pump_until(Duration::from_secs(5), |d| d.live_items(page).len() == 2)
        .await;
    let left = a.live_items(page);
    assert!(
        left.contains(&ink) && left.contains(&agent[0]),
        "ink and the promoted mark survive; only agent items go"
    );
}

/// H-9: restore with a missing seq / unknown page / below-floor seq is 404/410, never a silent empty page; a restore
/// can itself be undone by restoring again.
#[tokio::test]
#[ignore = "needs S4: POST /pages/{id}/restore?to_seq=N"]
async fn h09_restore_edge_cases_and_restore_then_undo_restore() {
    let server = TestServer::start().await;
    let mut a = server.tablet(1);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "P");
    a.draw(page, &line(10.0, 10.0, 10));
    converge(&server, &mut [&mut a]).await;
    let mid = server.head();
    a.draw(page, &line(10.0, 50.0, 10));
    converge(&server, &mut [&mut a]).await;
    let (k, v) = auth();
    let restore = |page: PageId, to: u64| {
        let url = format!("{}/pages/{page}/restore?to_seq={to}", base(&server));
        async move {
            client()
                .post(url)
                .header(k, v)
                .send()
                .await
                .expect("restore")
                .status()
                .as_u16()
        }
    };
    assert_eq!(
        restore(PageId::from_parts(1, 1), mid).await,
        404,
        "unknown page"
    );
    assert_eq!(
        restore(page, 123_456).await,
        404,
        "a seq that was never logged is not an empty page"
    );
    assert_eq!(restore(page, mid).await, 200);
    a.pump_until(Duration::from_secs(5), |d| d.live_items(page).len() == 1)
        .await;
    assert_eq!(a.live_items(page).len(), 1, "restored to the earlier state");
    let after_restore = server.head();
    assert_eq!(
        restore(page, after_restore - 1).await,
        200,
        "restore-the-restore brings the stroke back"
    );
    a.pump_until(Duration::from_secs(5), |d| d.live_items(page).len() == 2)
        .await;
    assert_eq!(a.live_items(page).len(), 2);
}
