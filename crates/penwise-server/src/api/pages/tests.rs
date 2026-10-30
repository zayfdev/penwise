//! `/pages/<id>/select`: the current page moves and every connected device is told (design-harness M3, hunt #17).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::routes;
use crate::testing::Rig;

#[tokio::test]
async fn select_sets_the_current_page_and_announces_it() {
    let mut rig = Rig::new();
    let page = rig.fixed_page().await;
    let _other = rig.fixed_page().await;
    let base = rig.serve(routes()).await;

    let response = reqwest::Client::new()
        .post(format!("{base}/pages/{page}/select"))
        .send()
        .await
        .expect("select");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("select json");
    assert_eq!(body["active"], page.to_string());

    assert_eq!(
        rig.storage.current_page().await.expect("current page"),
        Some(page)
    );
    let announced = rig.announced.lock().expect("announced").clone();
    assert_eq!(
        announced,
        vec![page],
        "the switch is broadcast to the devices"
    );
}

#[tokio::test]
async fn focus_announces_the_page_and_frame_and_refuses_an_unknown_frame() {
    use penwise_core::item::{FramePreset, FrameSpec, Layer, Z_STEP};

    let mut rig = Rig::new();
    let page = rig.fixed_page().await;
    let frame = rig.item_id();
    let spec = FrameSpec::new(
        "prj_site".to_owned(),
        "/".to_owned(),
        FramePreset::Desktop,
        None,
        None,
        0.0,
        0.0,
    )
    .expect("frame");
    rig.edit(page, spec.edit(frame, Layer::Ink, Z_STEP)).await;
    let base = rig.serve(routes()).await;
    let client = reqwest::Client::new();
    let url = format!("{base}/pages/{page}/focus");

    let fit_frame = client
        .post(&url)
        .json(&serde_json::json!({ "item": frame.to_hex() }))
        .send()
        .await
        .expect("focus");
    assert_eq!(fit_frame.status(), reqwest::StatusCode::OK);
    let fit_page = client.post(&url).send().await.expect("focus page");
    assert_eq!(fit_page.status(), reqwest::StatusCode::OK);
    let stranger = client
        .post(&url)
        .json(&serde_json::json!({ "item": "00".repeat(16) }))
        .send()
        .await
        .expect("focus stranger");
    assert_eq!(stranger.status(), reqwest::StatusCode::NOT_FOUND);

    let focused = rig.focused.lock().expect("focused").clone();
    assert_eq!(
        focused,
        vec![(page, Some(frame)), (page, None)],
        "a frame, then the page's content; the unknown frame moved nothing"
    );
}
