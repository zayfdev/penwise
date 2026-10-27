//! Feature routes end to end over the real server: pictures, the page PNG, handwriting recognition and search.
//! A tablet (testkit device) draws through `/sync`; the HTTP routes author server-side ops that must reach the
//! tablet's replica and agree with the server's tables.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::{Opts, TOKEN, TestServer, converge, line};
use penwise_core::item::{ItemKind, Layer, LayerSet};
use serde_json::{Value, json};

fn url(server: &TestServer, path: &str) -> String {
    format!("http://{}{path}", server.addr)
}

fn get(server: &TestServer, path: &str) -> reqwest::RequestBuilder {
    reqwest::Client::new()
        .get(url(server, path))
        .header("X-Penwise-Token", TOKEN)
}

fn post(server: &TestServer, path: &str) -> reqwest::RequestBuilder {
    reqwest::Client::new()
        .post(url(server, path))
        .header("X-Penwise-Token", TOKEN)
}

fn red_png() -> Vec<u8> {
    let mut pixmap = tiny_skia::Pixmap::new(4, 4).unwrap();
    pixmap.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
    pixmap.encode_png().unwrap()
}

#[tokio::test]
async fn a_placed_picture_reaches_the_tablet_renders_into_the_png_and_removes_cleanly() {
    let server = TestServer::start().await;
    let mut a = server.tablet(1);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "P");
    let ink = a.draw(page, &line(700.0, 700.0, 20));
    converge(&server, &mut [&mut a]).await;

    let stored: Value = post(&server, "/assets")
        .body(red_png())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let placed: Value = post(&server, &format!("/pages/{page}/images"))
        .json(&json!({"asset": stored["asset"], "x": 100.0, "y": 100.0, "w": 200.0, "h": 100.0, "label": "hero"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    converge(&server, &mut [&mut a]).await;
    assert_eq!(
        2,
        a.live_items(page).len(),
        "the picture is an item on the tablet too"
    );
    let scene = a.replica.page(page).unwrap().scene();
    let images = scene
        .render_order(LayerSet::ALL)
        .into_iter()
        .filter(|id| matches!(scene.get(*id).map(|i| &i.kind), Some(ItemKind::Image(_))))
        .count();
    assert_eq!(1, images);

    let png = get(&server, &format!("/page.png?page={page}"))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let rendered = tiny_skia::Pixmap::decode_png(&png).unwrap();
    let inside = rendered.pixel(200, 150).unwrap();
    assert_eq!((255, 0, 0), (inside.red(), inside.green(), inside.blue()));

    let removed: Value = reqwest::Client::new()
        .delete(url(&server, &format!("/pages/{page}/images")))
        .header("X-Penwise-Token", TOKEN)
        .json(&json!({"ids": [placed["id"]]}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(1, removed["removed"]);
    converge(&server, &mut [&mut a]).await;
    assert_eq!(
        vec![ink],
        a.live_items(page),
        "only the ink is left on the tablet"
    );
}

/// A stand-in for the local vision model: always answers with the same transcription and counts its calls.
async fn fake_model(answer: &'static str) -> (String, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let router = axum::Router::new().route(
        "/api/generate",
        axum::routing::post(move || {
            let seen = Arc::clone(&seen);
            async move {
                seen.fetch_add(1, Ordering::SeqCst);
                axum::Json(json!({ "response": answer }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await });
    (format!("http://{addr}"), calls)
}

#[tokio::test]
async fn recognized_text_syncs_to_the_tablet_and_is_searchable() {
    let (model, calls) = fake_model("grocery list: oat milk").await;
    let server = TestServer::start_with(Opts {
        ocr_url: Some(model),
        ..Opts::default()
    })
    .await;
    let mut a = server.tablet(1);
    let nb = a.create_notebook("N");
    let page = a.create_page(nb, "Kitchen");
    let first = a.draw(page, &line(100.0, 100.0, 30));
    converge(&server, &mut [&mut a]).await;

    let body: Value = get(&server, &format!("/text?page={page}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!("grocery list: oat milk", body["text"]);
    converge(&server, &mut [&mut a]).await;
    let scene = a.replica.page(page).unwrap().scene();
    let texts: Vec<_> = a
        .live_items(page)
        .into_iter()
        .filter_map(|id| scene.get(id))
        .filter_map(|item| match &item.kind {
            ItemKind::Text(text) => Some((item.layer, text.content.clone(), text.sources.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        vec![(
            Layer::Text,
            "grocery list: oat milk".to_owned(),
            vec![first]
        )],
        texts,
        "the recognition is a text-layer item sourced from the stroke"
    );

    let hits: Value = get(&server, "/search?q=OAT")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(1, hits["hits"].as_array().unwrap().len());
    assert_eq!(page.to_string(), hits["hits"][0]["page"]);
    assert_eq!("Kitchen", hits["hits"][0]["title"]);

    // Cached until the ink changes: a second read does not call the model, a new stroke does.
    get(&server, &format!("/text?page={page}"))
        .send()
        .await
        .unwrap();
    assert_eq!(1, calls.load(Ordering::SeqCst));
    a.draw(page, &line(100.0, 300.0, 30));
    a.settle(Duration::from_secs(10)).await.unwrap();
    converge(&server, &mut [&mut a]).await;
    get(&server, &format!("/text?page={page}"))
        .send()
        .await
        .unwrap();
    assert_eq!(2, calls.load(Ordering::SeqCst));
    converge(&server, &mut [&mut a]).await;
}
