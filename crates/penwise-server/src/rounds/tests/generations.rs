//! Freezing a whole generation: the OG picture and one picture per variant, the wider column shift, the stored
//! rows and `GET /projects/<id>/history`. These take real snapshots, so they need Chromium (and report that they
//! were skipped without it).

use std::sync::Arc;

use penwise_core::edit::Edit;
use penwise_core::item::{FramePreset, ItemId, ItemKind, Z_STEP, set_chosen};
use penwise_core::marks::{MarkItem, MarkStatus};
use serde_json::{Value, json};

use super::{Board, board, get_json, group, mark, post, without_browser};
use crate::storage::GenerationRole::{Og, Variant};
use crate::testing::Rig;

const COLUMN: f32 = 1440.0 + 40.0;

fn chromium() -> bool {
    std::path::Path::new("/usr/bin/chromium").is_file()
}

/// A board with a marked, sent round 1 and `variants` variant frames (labelled A, B, C) beside its frame.
async fn board_with_variants(rig: &mut Rig, variants: usize) -> (Board, Vec<ItemId>) {
    let root = rig.dir.path().join("site");
    std::fs::create_dir_all(&root).unwrap();
    let board = board(rig, &root).await;
    mark(rig, &board).await;
    post(
        &format!("{}/projects/{}/rounds/send", board.base, board.project),
        json!({}),
    )
    .await;
    if variants == 0 {
        return (board, Vec::new());
    }
    let labels = ["A", "B", "C", "D"];
    let mut frames = propose(&board, &labels[..variants.max(2)]).await;
    if variants == 1 {
        // The proposal route wants two or more; the owner may delete one later.
        let doomed = frames.pop().unwrap();
        server_edits(rig, board.page, vec![Edit::Delete(doomed)]).await;
    }
    (board, frames)
}

/// A variant proposal beside the board's frame, one variant per label; returns the new frames in label order.
async fn propose(board: &Board, labels: &[&str]) -> Vec<ItemId> {
    let options: Vec<Value> = labels
        .iter()
        .map(|label| {
            let depth = 10 * (1 + u32::from(label.as_bytes()[0] - b'A'));
            json!({ "label": label, "patch": { "css": format!("h1 {{ color: red; margin-top: {depth}px }}") } })
        })
        .collect();
    let created: Value = post(
        &format!("{}/frames/{}/variants", board.base, board.frame.to_hex()),
        json!({ "variants": options }),
    )
    .await
    .json()
    .await
    .unwrap();
    created["frames"]
        .as_array()
        .unwrap_or_else(|| panic!("variants created: {created}"))
        .iter()
        .map(|id| ItemId::from_hex(id.as_str().unwrap()).unwrap())
        .collect()
}

/// Makes `frame` the owner's pick of its group.
async fn choose(rig: &Rig, page: penwise_proto::PageId, frame: ItemId) {
    let edits = {
        let view = rig.view(page).await;
        set_chosen(view.replica.scene(), frame, true).unwrap()
    };
    assert!(!edits.is_empty());
    server_edits(rig, page, edits).await;
}

/// Edits to existing items go through the server's own author, whose stamps follow the ones the variants were
/// created with (the rig device's clock is fixed and loses against them).
async fn server_edits(rig: &Rig, page: penwise_proto::PageId, edits: Vec<Edit>) {
    let mut author = crate::api::Author::new(&rig.storage);
    for edit in edits {
        author.item(page, edit).await.unwrap();
    }
    author.commit().await.unwrap();
}

fn resolve_url(board: &Board, n: u32) -> String {
    format!(
        "{}/projects/{}/rounds/{n}/resolve",
        board.base, board.project
    )
}

fn history_url(board: &Board, route: &str) -> String {
    format!(
        "{}/projects/{}/history?route={route}&preset=desktop",
        board.base, board.project
    )
}

/// `(x, y)` of every Image item on the board page, left to right.
async fn pictures(rig: &Rig, board: &Board) -> Vec<(ItemId, f32, f32)> {
    let view = rig.view(board.page).await;
    let scene = view.replica.scene();
    let mut out: Vec<(ItemId, f32, f32)> = scene
        .live_ids()
        .into_iter()
        .filter_map(|id| scene.get(id))
        .filter(|item| matches!(item.kind, ItemKind::Image(_)))
        .map(|item| (item.id, item.transform.e, item.transform.f))
        .collect();
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    out
}

async fn live_x(rig: &Rig, board: &Board) -> f32 {
    let view = rig.view(board.page).await;
    view.replica.scene().get(board.frame).unwrap().transform.e
}

#[tokio::test]
async fn a_round_with_no_variants_keeps_one_picture_and_one_column() {
    if !chromium() {
        eprintln!("chromium is not installed; generation test skipped");
        return;
    }
    let mut rig = Rig::new();
    let (board, _) = board_with_variants(&mut rig, 0).await;
    let reply: Value = post(&resolve_url(&board, 1), json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(reply["snapshots"].as_array().unwrap().len(), 1, "{reply}");
    let shown = pictures(&rig, &board).await;
    assert_eq!(shown.len(), 1);
    assert!(
        shown[0].1.abs() < 0.01,
        "the OG picture stands where the frame stood"
    );
    assert!(
        (live_x(&rig, &board).await - COLUMN).abs() < 0.01,
        "one column"
    );
    let history = get_json(&history_url(&board, "/")).await;
    let generations = history["generations"].as_array().unwrap();
    assert_eq!(generations.len(), 1, "{history}");
    assert_eq!(generations[0]["round"], 1);
    assert_eq!(generations[0]["variants"], json!([]));
    assert_eq!(generations[0]["og"]["item"], shown[0].0.to_hex());
    assert_eq!(generations[0]["og"]["w"], 1440);
    assert!(generations[0]["resolved_ms"].as_u64().unwrap() > 0);
    assert_eq!(
        (history["route"].as_str(), history["preset"].as_str()),
        (Some("/"), Some("desktop"))
    );
}

#[tokio::test]
async fn one_variant_is_pictured_beside_the_og_and_the_frame_moves_two_columns() {
    if !chromium() {
        eprintln!("chromium is not installed; generation test skipped");
        return;
    }
    let mut rig = Rig::new();
    let (board, variants) = board_with_variants(&mut rig, 1).await;
    assert_eq!(variants.len(), 1);
    let reply: Value = post(&resolve_url(&board, 1), json!({}))
        .await
        .json()
        .await
        .unwrap();
    let snapshots = reply["snapshots"].as_array().unwrap();
    assert_eq!(snapshots.len(), 2, "{reply}");
    assert_eq!(snapshots[0]["role"], "og");
    assert_eq!(snapshots[1]["role"], "variant");
    assert_eq!(snapshots[1]["variant"], "A");
    let shown = pictures(&rig, &board).await;
    assert_eq!(shown.len(), 2);
    assert!(
        shown[0].1.abs() < 0.01 && (shown[1].1 - COLUMN).abs() < 0.01,
        "{shown:?}"
    );
    assert!((shown[0].2 - shown[1].2).abs() < 0.01, "one row");
    assert!(
        (live_x(&rig, &board).await - 2.0 * COLUMN).abs() < 0.01,
        "two columns"
    );
    let history = get_json(&history_url(&board, "/")).await;
    assert_eq!(history["generations"][0]["variants"][0]["label"], "A");
    assert_eq!(
        history["generations"][0]["variants"][0]["picked"], false,
        "no pick"
    );
}

#[tokio::test]
async fn three_variants_come_in_label_order_and_the_chosen_one_is_marked_picked() {
    if !chromium() {
        eprintln!("chromium is not installed; generation test skipped");
        return;
    }
    let mut rig = Rig::new();
    let (board, variants) = board_with_variants(&mut rig, 3).await;
    let edits = {
        let view = rig.view(board.page).await;
        set_chosen(view.replica.scene(), variants[1], true).unwrap()
    };
    assert!(!edits.is_empty());
    server_edits(&rig, board.page, edits).await;
    let reply: Value = post(&resolve_url(&board, 1), json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(reply["snapshots"].as_array().unwrap().len(), 4, "{reply}");
    assert_eq!(reply["variants_removed"], 3);
    let shown = pictures(&rig, &board).await;
    let columns: Vec<f32> = shown.iter().map(|picture| picture.1).collect();
    assert_eq!(columns.len(), 4);
    for (index, x) in (0_u8..).zip(&columns) {
        assert!((x - f32::from(index) * COLUMN).abs() < 0.01, "{columns:?}");
    }
    assert!(
        (live_x(&rig, &board).await - 4.0 * COLUMN).abs() < 0.01,
        "four columns"
    );
    let history = get_json(&history_url(&board, "/")).await;
    let kept = history["generations"][0]["variants"].as_array().unwrap();
    let labels: Vec<&str> = kept.iter().map(|v| v["label"].as_str().unwrap()).collect();
    assert_eq!(labels, ["A", "B", "C"], "{history}");
    let picked: Vec<bool> = kept
        .iter()
        .map(|v| v["picked"].as_bool().unwrap())
        .collect();
    assert_eq!(picked, [false, true, false]);
    // Each variant is its own patched page, not a copy of the OG picture.
    let mut assets: Vec<&str> = std::iter::once(&history["generations"][0]["og"])
        .chain(kept.iter())
        .map(|v| v["asset"].as_str().unwrap())
        .collect();
    assets.sort_unstable();
    assets.dedup();
    assert_eq!(assets.len(), 4, "four different pictures: {history}");
    // The row order on the board matches the listing: OG, then A, B, C.
    let items: Vec<String> = shown.iter().map(|p| p.0.to_hex()).collect();
    let expected: Vec<&str> = std::iter::once(&history["generations"][0]["og"])
        .chain(kept.iter())
        .map(|v| v["item"].as_str().unwrap())
        .collect();
    assert_eq!(items, expected);
}

#[tokio::test]
async fn a_variant_marked_after_send_is_neither_pictured_nor_deleted() {
    if !chromium() {
        eprintln!("chromium is not installed; generation test skipped");
        return;
    }
    let mut rig = Rig::new();
    let (board, variants) = board_with_variants(&mut rig, 3).await;
    // The owner draws on variant B after Send: that mark belongs to the next round.
    let late = rig.item_id();
    rig.edit(
        board.page,
        Edit::Add(
            MarkItem {
                frame: variants[1],
                preset: FramePreset::Desktop,
                status: MarkStatus::New,
                state: String::new(),
                strokes: Vec::new(),
            }
            .item(late, Z_STEP * 3),
        ),
    )
    .await;
    // B is also the owner's pick, but it stays on the board and so is not recorded as picked in history.
    choose(&rig, board.page, variants[1]).await;
    let reply: Value = post(&resolve_url(&board, 1), json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        reply["snapshots"].as_array().unwrap().len(),
        3,
        "OG, A and C: {reply}"
    );
    assert_eq!(reply["variants_removed"], 2);
    let view = rig.view(board.page).await;
    assert!(
        view.replica.scene().get(variants[1]).is_some(),
        "B survives"
    );
    assert!(view.replica.scene().get(variants[0]).is_none());
    let history = get_json(&history_url(&board, "/")).await;
    let labels: Vec<&str> = history["generations"][0]["variants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["A", "C"], "{history}");
    assert!(
        history["generations"][0]["variants"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["picked"] == false),
        "a protected pick is recorded as no pick: {history}"
    );
    assert!(
        (live_x(&rig, &board).await - 3.0 * COLUMN).abs() < 0.01,
        "three pictures make three columns"
    );
}

#[tokio::test]
async fn history_lists_rounds_newest_first_per_lane_and_validates_its_query() {
    if !chromium() {
        eprintln!("chromium is not installed; generation test skipped");
        return;
    }
    let mut rig = Rig::new();
    let (board, _) = board_with_variants(&mut rig, 0).await;
    post(&resolve_url(&board, 1), json!({})).await;
    mark(&mut rig, &board).await;
    post(
        &format!("{}/projects/{}/rounds/send", board.base, board.project),
        json!({}),
    )
    .await;
    post(&resolve_url(&board, 2), json!({})).await;
    let history = get_json(&history_url(&board, "/")).await;
    let rounds: Vec<u64> = history["generations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["round"].as_u64().unwrap())
        .collect();
    assert_eq!(rounds, [2, 1], "{history}");
    let other = get_json(&history_url(&board, "/work")).await;
    assert_eq!(other["generations"], json!([]));
    let missing = reqwest::get(format!("{}/projects/{}/history", board.base, board.project))
        .await
        .unwrap();
    assert_eq!(missing.status(), reqwest::StatusCode::BAD_REQUEST);
    let bad = reqwest::get(format!(
        "{}/projects/{}/history?route=/&preset=watch",
        board.base, board.project
    ))
    .await
    .unwrap();
    assert_eq!(bad.status(), reqwest::StatusCode::BAD_REQUEST);
    let unknown = reqwest::get(format!("{}/projects/prj_nope/history?route=/", board.base))
        .await
        .unwrap();
    assert_eq!(unknown.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_503_then_a_retry_leaves_one_picture_and_one_row_per_frame() {
    if !chromium() {
        eprintln!("chromium is not installed; generation test skipped");
        return;
    }
    let mut rig = Rig::new();
    let (board, _) = board_with_variants(&mut rig, 2).await;
    let broken = rig
        .serve_app(group(Arc::new(String::new())), without_browser(rig.app()))
        .await;
    let failed = post(
        &format!("{broken}/projects/{}/rounds/1/resolve", board.project),
        json!({}),
    )
    .await;
    assert_eq!(failed.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert!(pictures(&rig, &board).await.is_empty(), "nothing changed");
    assert_eq!(
        get_json(&history_url(&board, "/")).await["generations"],
        json!([])
    );
    post(&resolve_url(&board, 1), json!({})).await;
    let again: Value = post(&resolve_url(&board, 1), json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(again["already"], true);
    assert_eq!(pictures(&rig, &board).await.len(), 3, "OG, A, B once");
    let history = get_json(&history_url(&board, "/")).await;
    assert_eq!(history["generations"].as_array().unwrap().len(), 1);
    assert_eq!(
        history["generations"][0]["variants"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn two_variant_groups_with_the_same_labels_keep_all_their_pictures_and_both_picks() {
    if !chromium() {
        eprintln!("chromium is not installed; generation test skipped");
        return;
    }
    let mut rig = Rig::new();
    let (board, first) = board_with_variants(&mut rig, 2).await;
    let second = propose(&board, &["A", "B"]).await;
    choose(&rig, board.page, first[0]).await;
    choose(&rig, board.page, second[1]).await;
    let reply: Value = post(&resolve_url(&board, 1), json!({}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(reply["snapshots"].as_array().unwrap().len(), 5, "{reply}");
    assert_eq!(pictures(&rig, &board).await.len(), 5);
    let history = get_json(&history_url(&board, "/")).await;
    let kept = history["generations"][0]["variants"].as_array().unwrap();
    let labels: Vec<&str> = kept.iter().map(|v| v["label"].as_str().unwrap()).collect();
    assert_eq!(labels, ["A", "A", "B", "B"], "{history}");
    assert_eq!(kept.iter().filter(|v| v["picked"] == true).count(), 2);
    assert!(
        (live_x(&rig, &board).await - 5.0 * COLUMN).abs() < 0.01,
        "five pictures, five columns"
    );
}

#[tokio::test]
async fn a_retry_replaces_rows_a_failed_attempt_left_before_its_commit() {
    if !chromium() {
        eprintln!("chromium is not installed; generation test skipped");
        return;
    }
    let mut rig = Rig::new();
    let (board, variants) = board_with_variants(&mut rig, 3).await;
    // The first attempt got as far as recording its rows (its pictures were A, B and C), then failed before the
    // board commit; meanwhile the owner deleted B.
    let stale = |role, label: &str, item: &str| crate::storage::Generation {
        project: board.project.clone(),
        round: 1,
        route: "/".to_owned(),
        preset: "desktop".to_owned(),
        role,
        label: label.to_owned(),
        picked: false,
        item: format!("{item:f>32}"),
        asset: "0".repeat(64),
        w: 1440,
        h: 900,
        resolved_ms: 0,
    };
    rig.storage
        .record_generations(vec![
            stale(Og, "", "1"),
            stale(Variant, "A", "2"),
            stale(Variant, "B", "3"),
            stale(Variant, "C", "4"),
        ])
        .await
        .unwrap();
    server_edits(&rig, board.page, vec![Edit::Delete(variants[1])]).await;
    post(&resolve_url(&board, 1), json!({})).await;
    let history = get_json(&history_url(&board, "/")).await;
    let kept = history["generations"][0]["variants"].as_array().unwrap();
    let labels: Vec<&str> = kept.iter().map(|v| v["label"].as_str().unwrap()).collect();
    assert_eq!(labels, ["A", "C"], "no row for the deleted B: {history}");
    let on_board: Vec<String> = pictures(&rig, &board)
        .await
        .iter()
        .map(|p| p.0.to_hex())
        .collect();
    let rows: Vec<&str> = std::iter::once(&history["generations"][0]["og"])
        .chain(kept.iter())
        .map(|v| v["item"].as_str().unwrap())
        .collect();
    assert_eq!(on_board, rows, "every row is a picture on the board");
}

#[tokio::test]
async fn look_shows_the_history_line_of_a_lane_after_its_round_resolved() {
    if !chromium() {
        eprintln!("chromium is not installed; generation test skipped");
        return;
    }
    let mut rig = Rig::new();
    let (board, variants) = board_with_variants(&mut rig, 2).await;
    choose(&rig, board.page, variants[1]).await;
    post(&resolve_url(&board, 1), json!({})).await;
    // Round 2: a new mark on the live frame brings its lane into the look.
    mark(&mut rig, &board).await;
    let looked: Value = post(
        &format!("{}/agent/look", board.base),
        json!({ "project": board.project, "images": "none" }),
    )
    .await
    .json()
    .await
    .unwrap();
    let text = looked["text"].as_str().unwrap();
    assert!(
        text.contains("    history: round 1 picked \"B\" (2 variants)"),
        "{text}"
    );
}
