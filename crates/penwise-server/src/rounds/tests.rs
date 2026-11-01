//! HTTP tests for the round routes against the real project group on a loopback socket. They need no browser:
//! rounds are decided by the op-log, not by pixels.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

mod generations;
mod optional_body;
mod strip_pages;

use std::path::Path;
use std::sync::Arc;

use penwise_core::edit::Edit;
use penwise_core::item::{FramePreset, ItemId, Z_STEP};
use penwise_core::marks::{MarkItem, MarkStatus};
use penwise_proto::PageId;
use serde_json::{Value, json};

use crate::api::projects::routes as group;
use crate::storage::{RoundOp, RoundStatus};
use crate::testing::Rig;

async fn post(url: &str, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(url)
        .json(&body)
        .send()
        .await
        .expect("post")
}

async fn get_json(url: &str) -> Value {
    reqwest::get(url)
        .await
        .expect("get")
        .json()
        .await
        .expect("json")
}

/// A linked static project and its board page, which holds one desktop frame of it.
struct Board {
    base: String,
    project: String,
    page: PageId,
    frame: ItemId,
}

async fn board(rig: &mut Rig, root: &Path) -> Board {
    std::fs::write(root.join("index.html"), "<!doctype html><h1>hi</h1>").expect("index");
    let base = rig.serve(group(Arc::new(String::new()))).await;
    let linked: Value = post(
        &format!("{base}/projects"),
        json!({"name": "site", "root": root, "static": root}),
    )
    .await
    .json()
    .await
    .expect("project json");
    let project = linked["id"].as_str().expect("project id").to_owned();
    // The agent's own `show frame` makes the board in the project's notebook, where the ledger looks.
    let shown: Value = post(
        &format!("{base}/agent/show"),
        json!({"project": project, "frame": {"route": "/"}}),
    )
    .await
    .json()
    .await
    .expect("show json");
    Board {
        base,
        project,
        page: shown["page"].as_str().unwrap().parse().unwrap(),
        frame: ItemId::from_hex(shown["frame"].as_str().unwrap()).unwrap(),
    }
}

/// Adds one new mark on the board's frame; returns its id.
async fn mark(rig: &mut Rig, board: &Board) -> ItemId {
    let id = rig.item_id();
    let item = MarkItem {
        frame: board.frame,
        preset: FramePreset::Desktop,
        status: MarkStatus::New,
        state: String::new(),
        strokes: Vec::new(),
    }
    .item(id, Z_STEP * 2);
    rig.edit(board.page, Edit::Add(item)).await;
    id
}

#[tokio::test]
async fn send_closes_the_open_round_and_an_empty_one_is_a_conflict() {
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    let board = board(&mut rig, &root).await;
    let url = format!("{}/projects/{}/rounds/send", board.base, board.project);

    let empty = post(&url, json!({})).await;
    assert_eq!(empty.status(), reqwest::StatusCode::CONFLICT);

    mark(&mut rig, &board).await;
    mark(&mut rig, &board).await;
    let sent: Value = post(&url, json!({})).await.json().await.expect("send json");
    assert_eq!(sent, json!({"n": 1, "marks": 2}));

    let again = post(&url, json!({})).await;
    assert_eq!(
        again.status(),
        reqwest::StatusCode::CONFLICT,
        "the new open round has no marks yet"
    );
    let stale = post(&url, json!({ "n": 1 })).await;
    assert_eq!(
        stale.status(),
        reqwest::StatusCode::CONFLICT,
        "round 1 was already sent"
    );
    let unknown = post(
        &format!("{}/projects/prj_nope/rounds/send", board.base),
        json!({}),
    )
    .await;
    assert_eq!(unknown.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_round_holds_the_marks_added_since_the_previous_send() {
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    let board = board(&mut rig, &root).await;
    let send = format!("{}/projects/{}/rounds/send", board.base, board.project);
    let list = format!("{}/projects/{}/rounds", board.base, board.project);

    mark(&mut rig, &board).await;
    assert_eq!(
        get_json(&list).await["rounds"],
        json!([round(1, "open", 1, 1, 0)]),
        "before Send the open round shows what is drawn"
    );
    post(&send, json!({})).await;
    mark(&mut rig, &board).await;
    mark(&mut rig, &board).await;
    mark(&mut rig, &board).await;
    let rounds = get_json(&list).await["rounds"].clone();
    assert_eq!(rounds[0]["status"], "sent");
    assert_eq!(rounds[0]["marks"], 1, "round 1 keeps its one mark");
    assert_eq!(rounds[1]["n"], 2);
    assert_eq!(rounds[1]["status"], "open");
    assert_eq!(rounds[1]["marks"], 3, "round 2 is what came after the send");
    post(&send, json!({})).await;
    let rounds = get_json(&list).await["rounds"].clone();
    assert_eq!(rounds[0]["marks"], 1);
    assert_eq!(rounds[1]["marks"], 3);
    assert_eq!(rounds[1]["status"], "sent");
    assert_eq!(rounds[2]["n"], 3, "the next open round is listed");
    assert_eq!(rounds[2]["marks"], 0);
}

fn round(n: u32, status: &str, marks: u32, new: u32, done: u32) -> Value {
    json!({
        "n": n, "status": status, "marks": marks, "new": new, "seen": 0, "done": done, "reopened": 0,
        "sent_seq": null, "sent_ms": null, "resolved_ms": null, "looked_ms": null
    })
}

#[tokio::test]
async fn the_inbox_names_a_sent_round_until_the_agent_looks_at_it() {
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    let board = board(&mut rig, &root).await;
    let inbox = format!("{}/projects/{}/inbox", board.base, board.project);

    mark(&mut rig, &board).await;
    assert_eq!(
        get_json(&inbox).await["line"],
        Value::Null,
        "an open round is not announced"
    );
    post(
        &format!("{}/projects/{}/rounds/send", board.base, board.project),
        json!({}),
    )
    .await;
    assert_eq!(
        get_json(&inbox).await["line"],
        "Penwise: round 1 sent \u{2014} 1 mark on Desktop / (call look)"
    );

    rig.storage
        .round_op(RoundOp::Looked {
            project: board.project.clone(),
            n: 1,
        })
        .await
        .expect("looked");
    assert_eq!(get_json(&inbox).await["line"], Value::Null);

    // The next round: two marks, sent again, announced again.
    mark(&mut rig, &board).await;
    mark(&mut rig, &board).await;
    post(
        &format!("{}/projects/{}/rounds/send", board.base, board.project),
        json!({}),
    )
    .await;
    assert_eq!(
        get_json(&inbox).await["line"],
        "Penwise: round 2 sent \u{2014} 2 marks on Desktop / (call look)"
    );
    let unknown = get_json(&format!("{}/projects/prj_nope/inbox", board.base)).await;
    assert_eq!(
        unknown["line"],
        Value::Null,
        "an unknown project has nothing waiting"
    );
}

#[tokio::test]
async fn resolving_a_resolved_round_changes_nothing_and_an_unknown_one_is_a_404() {
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    let board = board(&mut rig, &root).await;
    let base = &board.base;
    let project = &board.project;
    let missing = post(
        &format!("{base}/projects/{project}/rounds/7/resolve"),
        json!({}),
    )
    .await;
    assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);

    // A resolved row without any frame work to do: mark the round resolved straight in storage.
    rig.storage
        .round_op(RoundOp::EnsureOpen {
            project: project.clone(),
        })
        .await
        .unwrap();
    let resolved = rig
        .storage
        .round_op(RoundOp::Resolve {
            project: project.clone(),
            n: 1,
        })
        .await
        .unwrap()
        .expect("round 1");
    assert_eq!(resolved.status, RoundStatus::Resolved);
    let again: Value = post(
        &format!("{base}/projects/{project}/rounds/1/resolve"),
        json!({}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(
        again,
        json!({"n": 1, "status": "resolved", "already": true})
    );
}

#[tokio::test]
async fn two_sends_of_the_same_round_have_one_winner() {
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    let board = board(&mut rig, &root).await;
    mark(&mut rig, &board).await;
    let url = format!("{}/projects/{}/rounds/send", board.base, board.project);
    let (first, second) =
        tokio::join!(post(&url, json!({ "n": 1 })), post(&url, json!({ "n": 1 })));
    let mut statuses = [first.status(), second.status()];
    statuses.sort();
    assert_eq!(
        statuses,
        [reqwest::StatusCode::OK, reqwest::StatusCode::CONFLICT],
        "exactly one send closes round 1"
    );
    let rounds = rig.storage.rounds(board.project.clone()).await.unwrap();
    assert_eq!(
        rounds
            .iter()
            .filter(|r| r.status == RoundStatus::Sent)
            .count(),
        1
    );
    assert_eq!(rounds[0].n, 1);
}

#[tokio::test]
async fn reopened_marks_count_in_the_inbox_until_the_agent_looks() {
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    let board = board(&mut rig, &root).await;
    let inbox = format!("{}/projects/{}/inbox", board.base, board.project);
    let send = format!("{}/projects/{}/rounds/send", board.base, board.project);

    // Round 1 is sent, looked at and its mark done; then the owner reopens that mark.
    let old = mark(&mut rig, &board).await;
    post(&send, json!({})).await;
    rig.storage
        .round_op(RoundOp::Looked {
            project: board.project.clone(),
            n: 1,
        })
        .await
        .unwrap();
    reopen(&mut rig, &board, old).await;
    assert_eq!(
        get_json(&inbox).await["line"],
        "Penwise: 1 mark reopened on Desktop / (call look)",
        "a reopened mark of an old round is announced on its own"
    );

    // A new round is sent as well: its marks and the reopened one are both in the line.
    mark(&mut rig, &board).await;
    mark(&mut rig, &board).await;
    post(&send, json!({})).await;
    assert_eq!(
        get_json(&inbox).await["line"],
        "Penwise: round 2 sent \u{2014} 2 marks (+1 reopened) on Desktop / (call look)"
    );

    // The agent looks: the checkpoint moves past the reopen and the round is looked at.
    let head = rig.storage.status().await.unwrap().head_seq;
    rig.storage
        .set_agent_checkpoint("agent".to_owned(), head)
        .await
        .unwrap();
    rig.storage
        .round_op(RoundOp::Looked {
            project: board.project.clone(),
            n: 2,
        })
        .await
        .unwrap();
    assert_eq!(get_json(&inbox).await["line"], Value::Null);
}

/// The owner reopens `mark`: an LWW status edit from the tablet.
async fn reopen(rig: &mut Rig, board: &Board, mark: ItemId) {
    let view = rig.view(board.page).await;
    let edits =
        penwise_core::marks::set_mark_status(view.replica.scene(), mark, MarkStatus::Reopened)
            .expect("status edit");
    for edit in edits {
        rig.edit(board.page, edit).await;
    }
}

#[tokio::test]
async fn the_ledger_reads_the_projects_own_pages_only() {
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    let board = board(&mut rig, &root).await;
    mark(&mut rig, &board).await;
    // A page of another notebook with a frame of the same project and a mark: not scanned.
    let elsewhere = rig.fixed_page().await;
    let frame = rig.item_id();
    let spec = penwise_core::item::FrameSpec::new(
        board.project.clone(),
        "/".to_owned(),
        FramePreset::Desktop,
        None,
        None,
        0.0,
        0.0,
    )
    .expect("frame");
    rig.edit(
        elsewhere,
        spec.edit(frame, penwise_core::item::Layer::Ink, Z_STEP),
    )
    .await;
    let stray = rig.item_id();
    let item = MarkItem {
        frame,
        preset: FramePreset::Desktop,
        status: MarkStatus::New,
        state: String::new(),
        strokes: Vec::new(),
    }
    .item(stray, Z_STEP * 2);
    rig.edit(elsewhere, Edit::Add(item)).await;
    let rounds = get_json(&format!("{}/projects/{}/rounds", board.base, board.project)).await;
    assert_eq!(rounds["rounds"][0]["marks"], 1, "only the board's mark");
}

/// The same app with a headless browser that cannot start: every snapshot is a 503.
fn without_browser(mut app: crate::api::AppState) -> crate::api::AppState {
    app.inspect = Arc::new(crate::inspect::Inspect::with_chromium(
        "/nonexistent/chromium",
    ));
    app
}

#[tokio::test]
async fn a_failed_freeze_has_already_sent_the_round_so_later_marks_belong_to_the_next_one() {
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    // The first server (a working app) links the project and makes the board.
    let board = board(&mut rig, &root).await;
    let first = mark(&mut rig, &board).await;
    // A second server on the same storage, whose browser cannot start, is asked to resolve the OPEN round 1.
    let broken = rig
        .serve_app(group(Arc::new(String::new())), without_browser(rig.app()))
        .await;
    let failed = post(
        &format!("{broken}/projects/{}/rounds/1/resolve", board.project),
        json!({}),
    )
    .await;
    assert_eq!(failed.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);

    // Nothing was frozen ...
    let view = rig.view(board.page).await;
    assert!(
        view.replica
            .scene()
            .live_ids()
            .into_iter()
            .filter_map(|id| view.replica.scene().get(id))
            .all(|item| !matches!(item.kind, penwise_core::item::ItemKind::Image(_))),
        "no picture was placed"
    );
    let moved = view.replica.scene().get(board.frame).unwrap().transform.e;
    assert!(moved.abs() < 0.01, "the frame did not move");
    // ... but the round was sent before the freeze began, so a mark drawn now is round 2's and still new.
    let late = mark(&mut rig, &board).await;
    let rounds = get_json(&format!("{}/projects/{}/rounds", board.base, board.project)).await;
    assert_eq!(rounds["rounds"][0]["status"], "sent");
    assert_eq!(rounds["rounds"][0]["marks"], 1);
    assert_eq!(rounds["rounds"][1]["n"], 2);
    assert_eq!(rounds["rounds"][1]["marks"], 1);
    assert_eq!(rounds["rounds"][1]["new"], 1);
    let view = rig.view(board.page).await;
    let status = |id| {
        penwise_core::marks::mark_of(view.replica.scene(), id)
            .unwrap()
            .status
    };
    assert_eq!(status(first), MarkStatus::New);
    assert_eq!(status(late), MarkStatus::New);
}

/// Adds a history picture for round 1 of the board's frame, as a committed freeze would have.
async fn plant_picture(rig: &mut Rig, board: &Board) -> ItemId {
    use penwise_core::item::{Affine, ImageItem, Item, ItemKind, Layer};
    let stored = rig.picture(40, 20, [1, 2, 3, 255]);
    let id = rig.item_id();
    let item = Item {
        id,
        layer: Layer::Ink,
        z: -Z_STEP,
        transform: Affine::translate(0.0, 0.0),
        kind: ItemKind::Image(ImageItem {
            asset: crate::assets::digest_of(&stored.asset).unwrap(),
            w: 1440.0,
            h: 900.0,
        }),
    };
    rig.edit(board.page, Edit::Add(item)).await;
    crate::pictures::store_label(&rig.app(), id, "Round 1 \u{2014} desktop /".to_owned()).await;
    id
}

#[tokio::test]
async fn a_frame_that_already_has_the_rounds_picture_is_not_frozen_again() {
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    let board = board(&mut rig, &root).await;
    let first = mark(&mut rig, &board).await;
    post(
        &format!("{}/projects/{}/rounds/send", board.base, board.project),
        json!({}),
    )
    .await;
    // A retry after a partial commit: the picture is there, the frame is not touched again. With no browser the
    // call could not snapshot anything, so success proves nothing was attempted.
    plant_picture(&mut rig, &board).await;
    let broken = rig
        .serve_app(group(Arc::new(String::new())), without_browser(rig.app()))
        .await;
    let reply = post(
        &format!("{broken}/projects/{}/rounds/1/resolve", board.project),
        json!({}),
    )
    .await;
    assert_eq!(reply.status(), reqwest::StatusCode::OK);
    let reply: Value = reply.json().await.unwrap();
    assert_eq!(reply["status"], "resolved");
    assert_eq!(reply["already"], 1);
    assert_eq!(reply["snapshots"], json!([]));
    let view = rig.view(board.page).await;
    let scene = view.replica.scene();
    assert!(
        scene.get(board.frame).unwrap().transform.e.abs() < 0.01,
        "the frame stays put"
    );
    let pictures = scene
        .live_ids()
        .into_iter()
        .filter_map(|id| scene.get(id))
        .filter(|item| matches!(item.kind, penwise_core::item::ItemKind::Image(_)))
        .count();
    assert_eq!(pictures, 1, "still one picture");
    assert_eq!(
        penwise_core::marks::mark_of(scene, first).unwrap().status,
        MarkStatus::Done
    );
}

#[tokio::test]
async fn a_503_changes_nothing_and_the_retry_leaves_one_picture_per_frame() {
    if !std::path::Path::new("/usr/bin/chromium").is_file() {
        eprintln!("chromium is not installed; retry test skipped");
        return;
    }
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    let board = board(&mut rig, &root).await;
    mark(&mut rig, &board).await;
    post(
        &format!("{}/projects/{}/rounds/send", board.base, board.project),
        json!({}),
    )
    .await;
    let broken = rig
        .serve_app(group(Arc::new(String::new())), without_browser(rig.app()))
        .await;
    let url = |base: &str| format!("{base}/projects/{}/rounds/1/resolve", board.project);
    assert_eq!(
        post(&url(&broken), json!({})).await.status(),
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    // Retry with a working browser, then once more: one picture, the second call is a no-op.
    let working = post(&url(&board.base), json!({})).await;
    assert_eq!(working.status(), reqwest::StatusCode::OK);
    let again: Value = post(&url(&board.base), json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(again["already"], true);
    let view = rig.view(board.page).await;
    let scene = view.replica.scene();
    let count = scene
        .live_ids()
        .into_iter()
        .filter_map(|id| scene.get(id))
        .filter(|item| matches!(item.kind, penwise_core::item::ItemKind::Image(_)))
        .count();
    assert_eq!(count, 1, "one picture for the one frame");
}

#[tokio::test]
async fn a_frame_too_tall_for_the_canvas_is_pictured_at_its_full_box_size() {
    if !std::path::Path::new("/usr/bin/chromium").is_file() {
        eprintln!("chromium is not installed; scaled snapshot test skipped");
        return;
    }
    let mut rig = Rig::new();
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    // 1440 x 28 000 = 40.3 Mpx: over the snapshot's 40 Mpx canvas, so the snapshot comes back scaled down.
    let board = board(&mut rig, &root).await;
    std::fs::write(
        root.join("index.html"),
        "<!doctype html><body style=\"margin:0\"><div style=\"height:28000px;background:#eee\">tall</div>",
    )
    .unwrap();
    mark(&mut rig, &board).await;
    post(
        &format!("{}/projects/{}/rounds/send", board.base, board.project),
        json!({}),
    )
    .await;
    let reply: Value = post(
        &format!("{}/projects/{}/rounds/1/resolve", board.base, board.project),
        json!({}),
    )
    .await
    .json()
    .await
    .unwrap();
    let snapshots = reply["snapshots"].as_array().expect("snapshots");
    assert_eq!(snapshots.len(), 1, "{reply}");
    assert_eq!(
        snapshots[0]["w"], 1440.0,
        "the frame's width, not the pixel width"
    );
    let height = snapshots[0]["h"].as_f64().unwrap();
    assert!(
        (27_900.0..=28_100.0).contains(&height),
        "the document's height in CSS px, not the scaled pixel height: {reply}"
    );
}
