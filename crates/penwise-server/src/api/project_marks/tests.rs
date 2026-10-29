//! HTTP tests for `GET /pages/<id>/marks` and `POST /marks/<id>/status`, against the real router.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

use penwise_core::edit::Edit;
use penwise_core::item::{FramePreset, FrameSpec, ItemId, Layer, Z_STEP};
use penwise_core::marks::{MarkItem, MarkStatus, mark_of};
use penwise_proto::PageId;
use serde_json::{Value, json};

use std::sync::Arc;

use super::super::projects::routes as group;
use super::routes;
use crate::testing::Rig;

async fn get(url: &str) -> reqwest::Response {
    reqwest::get(url).await.expect("get")
}

async fn post(url: &str, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(url)
        .json(&body)
        .send()
        .await
        .expect("post")
}

/// A page with a phone frame and one mark (state + status New, no strokes yet).
async fn page_with_mark(rig: &mut Rig) -> (PageId, ItemId, ItemId) {
    let page = rig.fixed_page().await;
    let frame = rig.item_id();
    let spec = FrameSpec::new(
        "prj_site".to_owned(),
        "/work?tag=one".to_owned(),
        FramePreset::Phone,
        None,
        None,
        10.0,
        20.0,
    )
    .expect("frame");
    rig.edit(page, spec.edit(frame, Layer::Ink, Z_STEP)).await;
    let mark = rig.item_id();
    let item = MarkItem {
        frame,
        preset: FramePreset::Phone,
        status: MarkStatus::New,
        state: "click:#menu-button".to_owned(),
        strokes: Vec::new(),
    }
    .item(mark, Z_STEP * 2);
    rig.edit(page, Edit::Add(item)).await;
    (page, frame, mark)
}

#[tokio::test]
async fn list_returns_marks_with_their_frame_project_and_route() {
    let mut rig = Rig::new();
    let (page, frame, mark) = page_with_mark(&mut rig).await;
    // Served through the `/projects` group: same routes and the same loopback rule as `/projects`.
    let base = rig.serve(group(Arc::new(String::new()))).await;

    let response = get(&format!("{base}/pages/{page}/marks")).await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body: Value = response.json().await.expect("marks json");
    let marks = body["marks"].as_array().expect("marks array");
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0]["id"], mark.to_hex());
    assert_eq!(marks[0]["frame"], frame.to_hex());
    assert_eq!(marks[0]["project"], "prj_site");
    assert_eq!(marks[0]["route"], "/work?tag=one");
    assert_eq!(marks[0]["preset"], "phone");
    assert_eq!(marks[0]["status"], "new");
    assert_eq!(marks[0]["state"], "click:#menu-button");
    assert_eq!(marks[0]["strokes"], json!([]));
    assert_eq!(marks[0]["rect"], Value::Null, "no strokes, nothing visible");

    let all = get(&format!("{base}/pages/{page}/marks?status=all")).await;
    assert_eq!(all.status(), reqwest::StatusCode::OK);
    let new = get(&format!("{base}/pages/{page}/marks?status=new")).await;
    assert_eq!(new.status(), reqwest::StatusCode::OK);
    let done: Value = get(&format!("{base}/pages/{page}/marks?status=done"))
        .await
        .json()
        .await
        .expect("done json");
    assert_eq!(done["marks"], json!([]));
    assert_eq!(
        get(&format!("{base}/pages/{page}/marks?status=seen"))
            .await
            .status(),
        reqwest::StatusCode::OK
    );

    let bad = get(&format!("{base}/pages/{page}/marks?status=archived")).await;
    assert_eq!(bad.status(), reqwest::StatusCode::BAD_REQUEST);
    let unknown = get(&format!("{base}/pages/00000000000000000000000000/marks")).await;
    assert_eq!(unknown.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn status_is_committed_as_an_undoable_edit_and_is_idempotent() {
    let mut rig = Rig::new();
    let (page, _frame, mark) = page_with_mark(&mut rig).await;
    let base = rig.serve(routes()).await;
    let url = format!("{base}/marks/{}/status", mark.to_hex());

    let response = post(&url, json!({"status": "done"})).await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let body: Value = response.json().await.expect("status json");
    assert_eq!(body["id"], mark.to_hex());
    assert_eq!(body["status"], "done");

    let view = rig.view(page).await;
    let stored = view.replica.scene().get(mark).expect("mark item");
    let penwise_core::item::ItemKind::Mark(item) = &stored.kind else {
        panic!("not a mark");
    };
    assert_eq!(item.status, MarkStatus::Done);

    let done: Value = get(&format!("{base}/pages/{page}/marks?status=done"))
        .await
        .json()
        .await
        .expect("done json");
    assert_eq!(done["marks"][0]["status"], "done");
    assert_eq!(
        done["marks"][0]["rect"],
        Value::Null,
        "done marks stay hidden without a show-done view"
    );

    // Repeating the same status writes no second op.
    let before = rig.storage.status().await.expect("status").head_seq;
    let again = post(&url, json!({"status": "done"})).await;
    assert_eq!(again.status(), reqwest::StatusCode::OK);
    assert_eq!(
        rig.storage.status().await.expect("status").head_seq,
        before,
        "idempotent"
    );

    let reopened = post(&url, json!({"status": "reopened"})).await;
    assert_eq!(reopened.status(), reqwest::StatusCode::OK);
    let view = rig.view(page).await;
    assert_eq!(
        mark_of(view.replica.scene(), mark).expect("mark").status,
        MarkStatus::Reopened
    );
}

#[tokio::test]
async fn status_rejects_bad_input_and_unknown_marks() {
    let mut rig = Rig::new();
    let (_page, frame, mark) = page_with_mark(&mut rig).await;
    let base = rig.serve(routes()).await;
    let url = format!("{base}/marks/{}/status", mark.to_hex());

    assert_eq!(
        post(&url, json!({"status": "archived"})).await.status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post(
            &format!("{base}/marks/not-hex/status"),
            json!({"status": "seen"})
        )
        .await
        .status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post(
            &format!("{base}/marks/{}/status", "0".repeat(32)),
            json!({"status": "seen"})
        )
        .await
        .status(),
        reqwest::StatusCode::NOT_FOUND
    );
    // A frame id is not a mark.
    assert_eq!(
        post(
            &format!("{base}/marks/{}/status", frame.to_hex()),
            json!({"status": "seen"})
        )
        .await
        .status(),
        reqwest::StatusCode::NOT_FOUND
    );
}
