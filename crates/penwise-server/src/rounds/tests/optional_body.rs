//! The send body is optional: the tablet's empty POST arrives as `application/x-www-form-urlencoded`.

use serde_json::{Value, json};

use super::{Board, board, mark, post};
use crate::testing::Rig;

async fn raw(url: &str, content_type: Option<&str>, body: &'static str) -> reqwest::Response {
    let mut request = reqwest::Client::new().post(url).body(body);
    if let Some(kind) = content_type {
        request = request.header("content-type", kind);
    }
    request.send().await.expect("post")
}

#[tokio::test]
async fn send_takes_an_empty_body_under_any_content_type() {
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    let board: Board = board(&mut rig, &root).await;
    let url = format!("{}/projects/{}/rounds/send", board.base, board.project);
    let form = Some("application/x-www-form-urlencoded");

    // Handled (the open round is empty), not rejected with 415.
    assert_eq!(
        raw(&url, form, "").await.status(),
        reqwest::StatusCode::CONFLICT
    );
    assert_eq!(
        raw(&url, None, "").await.status(),
        reqwest::StatusCode::CONFLICT
    );
    let unknown = format!("{}/projects/prj_nope/rounds/send", board.base);
    assert_eq!(
        raw(&unknown, form, "").await.status(),
        reqwest::StatusCode::NOT_FOUND
    );

    assert_eq!(
        raw(&url, Some("application/json"), "{not json")
            .await
            .status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        raw(&url, form, "n=1").await.status(),
        reqwest::StatusCode::BAD_REQUEST
    );

    mark(&mut rig, &board).await;
    let sent = raw(&url, form, "").await;
    assert_eq!(sent.status(), reqwest::StatusCode::OK);
    assert_eq!(
        sent.json::<Value>().await.unwrap(),
        json!({"n": 1, "marks": 1})
    );

    // A JSON body with `n` still counts: round 1 is closed now.
    let stale = post(&url, json!({ "n": 1 })).await;
    assert_eq!(stale.status(), reqwest::StatusCode::CONFLICT);
}
