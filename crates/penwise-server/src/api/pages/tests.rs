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
