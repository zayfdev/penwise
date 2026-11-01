//! Strip pages: which sketch pages hang off which screen's strip. No browser needed.

use penwise_proto::{Background, PageKind};
use serde_json::{Value, json};

use super::{board, get_json};
use crate::testing::Rig;

async fn put(url: &str, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .put(url)
        .json(&body)
        .send()
        .await
        .expect("put")
}

#[tokio::test]
async fn pages_attach_idempotently_list_in_attach_order_and_drop_out_when_deleted() {
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    let board = board(&mut rig, &root).await;
    let notebook = rig.view(board.page).await.page.notebook;
    let first = rig.page_in(notebook, PageKind::Infinite).await;
    let second = rig.page_in(notebook, PageKind::Infinite).await;
    let url = |page| {
        format!(
            "{}/projects/{}/strip-pages/{page}",
            board.base, board.project
        )
    };
    let list = format!("{}/projects/{}/strip-pages", board.base, board.project);
    assert_eq!(get_json(&list).await, json!([]));

    let strip = json!({ "route": "/", "preset": "desktop" });
    let reply = put(&url(second), strip.clone()).await;
    assert_eq!(reply.status(), reqwest::StatusCode::OK);
    put(&url(first), json!({ "route": "/", "preset": "phone" })).await;
    // Attaching again changes nothing: no duplicate and no new place in the order.
    put(&url(second), strip.clone()).await;
    let attached = get_json(&list).await;
    assert_eq!(
        attached,
        json!([
            { "page": second.to_string(), "route": "/", "preset": "desktop" },
            { "page": first.to_string(), "route": "/", "preset": "phone" },
        ])
    );
    // Attaching to another strip moves the page, keeping its place.
    put(
        &url(second),
        json!({ "route": "/work", "preset": "desktop" }),
    )
    .await;
    assert_eq!(get_json(&list).await[0]["route"], "/work");

    rig.delete_page(second).await;
    let attached = get_json(&list).await;
    assert_eq!(attached.as_array().unwrap().len(), 1, "{attached}");
    assert_eq!(attached[0]["page"], first.to_string());
}

#[tokio::test]
async fn attaching_validates_the_project_the_page_and_the_strip() {
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    let board = board(&mut rig, &root).await;
    let notebook = rig.view(board.page).await.page.notebook;
    let page = rig.page_in(notebook, PageKind::Infinite).await;
    let strip = json!({ "route": "/", "preset": "desktop" });
    let status = |reply: reqwest::Response| reply.status();
    let at =
        |project: &str, page: &str| format!("{}/projects/{project}/strip-pages/{page}", board.base);
    assert_eq!(
        status(put(&at("prj_nope", &page.to_string()), strip.clone()).await),
        reqwest::StatusCode::NOT_FOUND
    );
    let unknown = penwise_proto::PageId([7; 16]).to_string();
    assert_eq!(
        status(put(&at(&board.project, &unknown), strip.clone()).await),
        reqwest::StatusCode::NOT_FOUND
    );
    // The board itself, and a page of some other notebook, are not strip pages.
    assert_eq!(
        status(put(&at(&board.project, &board.page.to_string()), strip.clone()).await),
        reqwest::StatusCode::BAD_REQUEST
    );
    let elsewhere = rig.page(PageKind::Infinite, Background::Blank).await;
    assert_eq!(
        status(put(&at(&board.project, &elsewhere.to_string()), strip.clone()).await),
        reqwest::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status(put(&at(&board.project, "not-a-page"), strip.clone()).await),
        reqwest::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status(
            put(
                &at(&board.project, &page.to_string()),
                json!({ "route": "/", "preset": "watch" })
            )
            .await
        ),
        reqwest::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status(
            put(
                &at(&board.project, &page.to_string()),
                json!({ "route": "", "preset": "desktop" })
            )
            .await
        ),
        reqwest::StatusCode::BAD_REQUEST
    );
}
