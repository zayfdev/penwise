//! Library behaviour: creation, migration, page CRUD, ops and frame shapes.

use super::*;
use crate::library::model::MAX_TITLE_BYTES;

fn stroke() -> StrokeMsg {
    StrokeMsg {
        layer: "ink".to_owned(),
        preset: 0,
        argb: -16_777_216,
        scale: 1.0,
        seed: "42".to_owned(),
        samples: vec![0.0, 0.0, 0.5, 0.0, 0.0, 10.0, 10.0, 0.5, 0.0, 0.0],
        times: vec![0, 8_000],
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("penwise-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[tokio::test]
async fn first_run_creates_one_notebook_and_one_page() {
    let dir = temp_dir("fresh");
    let library = Library::load(dir.clone()).await;
    let meta = library.meta().await;
    assert_eq!(1, meta.notebooks.len());
    assert_eq!(1, meta.notebooks[0].pages.len());
    assert_eq!(meta.notebooks[0].pages[0].id, meta.active);
    assert!(dir.join("meta.json").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_legacy_page_json_migrates_into_page_one() {
    let dir = temp_dir("legacy");
    std::fs::create_dir_all(&dir).unwrap();
    let legacy = serde_json::json!({ "version": 1, "strokes": [stroke()] });
    std::fs::write(dir.join("page.json"), legacy.to_string()).unwrap();
    let library = Library::load(dir.clone()).await;
    let active = library.active().await;
    assert_eq!(1, library.strokes(&active).await.len(), "strokes moved");
    assert!(dir.join("page.json").is_file(), "legacy file left in place");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn pages_created_after_the_first_are_numbered_and_persisted() {
    let dir = temp_dir("create");
    let library = Library::load(dir.clone()).await;
    let page = library
        .create_page(NewPage {
            background: Some(Background::Grid),
            ..NewPage::default()
        })
        .await
        .unwrap();
    assert_eq!("Page 2", page.title);
    assert_eq!(Background::Grid, page.background);
    assert_eq!(PageKind::a4(), page.kind);
    assert!(library.strokes(&page.id).await.is_empty());
    // A fresh load sees both pages and the stored background.
    drop(library);
    let reloaded = Library::load(dir.clone()).await;
    let meta = reloaded.meta().await;
    assert_eq!(2, meta.notebooks[0].pages.len());
    assert_eq!(
        Some(&Background::Grid),
        meta.page(&page.id).map(|p| &p.background)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn strokes_survive_a_reload_per_page() {
    let dir = temp_dir("strokes");
    let library = Library::load(dir.clone()).await;
    let first = library.active().await;
    let second = library.create_page(NewPage::default()).await.unwrap();
    assert!(library.append(&first, stroke()).await);
    assert!(library.append(&second.id, stroke()).await);
    assert!(library.append(&second.id, stroke()).await);
    drop(library);
    let reloaded = Library::load(dir.clone()).await;
    assert_eq!(1, reloaded.strokes(&first).await.len());
    assert_eq!(2, reloaded.strokes(&second.id).await.len());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn the_last_page_cannot_be_deleted() {
    let dir = temp_dir("delete");
    let library = Library::load(dir.clone()).await;
    let only = library.active().await;
    assert!(library.delete_page(&only).await.is_err());
    let second = library.create_page(NewPage::default()).await.unwrap();
    let meta = library.delete_page(&second.id).await.unwrap();
    assert_eq!(1, meta.notebooks[0].pages.len());
    assert_eq!(only, meta.active, "active page follows a deletion");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn deleting_the_active_page_moves_active_to_a_survivor() {
    let dir = temp_dir("delete-active");
    let library = Library::load(dir.clone()).await;
    let second = library.create_page(NewPage::default()).await.unwrap();
    assert!(library.select(&second.id).await);
    let meta = library.delete_page(&second.id).await.unwrap();
    assert_ne!(second.id, meta.active);
    assert!(meta.page(&meta.active).is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_patch_updates_title_and_background() {
    let dir = temp_dir("patch");
    let library = Library::load(dir.clone()).await;
    let active = library.active().await;
    let page = library
        .patch_page(
            &active,
            PagePatch {
                title: Some("Groceries".to_owned()),
                background: Some(Background::Ruled),
            },
        )
        .await
        .unwrap();
    assert_eq!("Groceries", page.title);
    assert_eq!(Background::Ruled, page.background);
    assert!(
        library
            .patch_page("pg_missing", PagePatch::default())
            .await
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn agent_marks_are_cleared_without_touching_ink() {
    let dir = temp_dir("agent");
    let library = Library::load(dir.clone()).await;
    let page = library.active().await;
    library.append(&page, stroke()).await;
    let mut mark = stroke();
    mark.layer = AGENT_LAYER.to_owned();
    library.append_many(&page, vec![mark]).await;
    assert_eq!(2, library.strokes(&page).await.len());
    assert_eq!(1, library.clear_agent(&page).await);
    assert_eq!(1, library.strokes(&page).await.len());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn removing_strokes_by_seed_broadcasts_only_the_removal() {
    let dir = temp_dir("op-remove");
    let library = Library::load(dir.clone()).await;
    let page = library.active().await;
    let mut first = stroke();
    let mut second = stroke();
    first.seed = "1".to_owned();
    second.seed = "2".to_owned();
    library.append_many(&page, vec![first, second]).await;
    let mut events = library.subscribe();
    assert_eq!(1, library.remove_strokes(&page, &["1".to_owned()]).await);
    let left = library.strokes(&page).await;
    assert_eq!(1, left.len());
    assert_eq!("2", left[0].seed);
    let frame = events.recv().await.unwrap();
    assert!(frame.text.contains("\"t\":\"op\""), "op frame");
    assert!(frame.text.contains("\"k\":\"remove\""), "removal kind");
    assert!(!frame.text.contains("\"strokes\""), "no full snapshot");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn clearing_a_page_empties_it_and_broadcasts_a_clear() {
    let dir = temp_dir("op-clear");
    let library = Library::load(dir.clone()).await;
    let page = library.active().await;
    library.append_many(&page, vec![stroke(), stroke()]).await;
    let mut events = library.subscribe();
    assert_eq!(2, library.clear_page(&page).await);
    assert!(library.strokes(&page).await.is_empty());
    let frame = events.recv().await.unwrap();
    assert!(frame.text.contains("\"k\":\"clear\""));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn replacing_a_stroke_swaps_it_for_the_fragments() {
    let dir = temp_dir("op-replace");
    let library = Library::load(dir.clone()).await;
    let page = library.active().await;
    let mut original = stroke();
    original.seed = "5".to_owned();
    library.append_many(&page, vec![original]).await;
    let mut left = stroke();
    left.seed = "6".to_owned();
    let mut right = stroke();
    right.seed = "7".to_owned();
    library
        .replace_strokes(&page, &["5".to_owned()], vec![left, right])
        .await;
    let seeds: Vec<String> = library
        .strokes(&page)
        .await
        .iter()
        .map(|s| s.seed.clone())
        .collect();
    assert_eq!(vec!["6".to_owned(), "7".to_owned()], seeds);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn frames_carry_the_page_they_belong_to() {
    let dir = temp_dir("frames");
    let library = Library::load(dir.clone()).await;
    let page = library.active().await;
    let mut events = library.subscribe();
    library.append(&page, stroke()).await;
    let frame = events.recv().await.unwrap();
    assert_eq!(Some(page.clone()), frame.page);
    assert!(frame.text.contains("\"t\":\"stroke\""));
    let mut events = library.subscribe();
    library.create_page(NewPage::default()).await.unwrap();
    let frame = events.recv().await.unwrap();
    assert_eq!(None, frame.page, "library changes are global");
    assert!(frame.text.contains("\"t\":\"meta\""));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn malformed_strokes_and_unknown_pages_are_refused() {
    let dir = temp_dir("refuse");
    let library = Library::load(dir.clone()).await;
    let page = library.active().await;
    let mut broken = stroke();
    broken.times.clear();
    assert!(!library.append(&page, broken).await);
    assert!(!library.append("pg_missing", stroke()).await);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ids_are_prefixed_and_unique() {
    let a = new_id("pg");
    let b = new_id("pg");
    assert!(a.starts_with("pg_"));
    assert_eq!(19, a.len());
    assert_ne!(a, b);
}

#[test]
fn client_ids_are_accepted_only_in_the_expected_shape() {
    assert!(is_page_id("pg_0123456789abcdef"));
    assert!(!is_page_id("pg_short"));
    assert!(!is_page_id("nb_0123456789abcdef"));
    assert!(!is_page_id("pg_0123456789ABCDEF"));
}

#[tokio::test]
async fn a_client_supplied_id_is_kept_and_is_idempotent() {
    let dir = temp_dir("client-id");
    let library = Library::load(dir.clone()).await;
    let request = || NewPage {
        id: Some("pg_00000000deadbeef".to_owned()),
        ..NewPage::default()
    };
    let page = library.create_page(request()).await.unwrap();
    assert_eq!("pg_00000000deadbeef", page.id);
    let again = library.create_page(request()).await.unwrap();
    assert_eq!(page.id, again.id);
    assert_eq!(2, library.meta().await.notebooks[0].pages.len());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn notebooks_can_be_created_renamed_and_emptied() {
    let dir = temp_dir("notebooks");
    let library = Library::load(dir.clone()).await;
    let (book, meta) = library.create_notebook("Recipes".to_owned()).await.unwrap();
    assert_eq!(2, meta.notebooks.len());
    assert!(book.pages.is_empty());
    let meta = library
        .rename_notebook(&book.id, "Recipes 2".to_owned())
        .await
        .unwrap();
    assert!(meta.notebooks.iter().any(|n| n.title == "Recipes 2"));
    let page = library
        .create_page(NewPage {
            notebook: Some(book.id.clone()),
            ..NewPage::default()
        })
        .await
        .unwrap();
    assert!(
        library.delete_notebook(&book.id).await.is_err(),
        "not empty"
    );
    library.delete_page(&page.id).await.unwrap();
    let meta = library.delete_notebook(&book.id).await.unwrap();
    assert_eq!(1, meta.notebooks.len());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn the_last_notebook_cannot_be_deleted() {
    let dir = temp_dir("last-notebook");
    let library = Library::load(dir.clone()).await;
    let only = library.meta().await.notebooks[0].id.clone();
    assert!(library.delete_notebook(&only).await.is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn titles_are_bounded() {
    assert!(check_title("ok").is_ok());
    assert!(check_title("   ").is_err());
    assert!(check_title(&"x".repeat(MAX_TITLE_BYTES + 1)).is_err());
}
