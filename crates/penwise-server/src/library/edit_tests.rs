//! Destructive edits: restore as undo, database-first ordering, seed dedupe.

use super::*;

fn stroke(seed: &str, layer: &str) -> StrokeMsg {
    StrokeMsg {
        layer: layer.to_owned(),
        preset: 0,
        argb: -16_777_216,
        scale: 1.0,
        seed: seed.to_owned(),
        samples: vec![0.0, 0.0, 0.5, 0.0, 0.0, 10.0, 10.0, 0.5, 0.0, 0.0],
        times: vec![0, 8_000],
    }
}

fn ink(seed: &str) -> StrokeMsg {
    stroke(seed, "ink")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("penwise-edit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn seeds(strokes: &[StrokeMsg]) -> Vec<&str> {
    strokes.iter().map(|stroke| stroke.seed.as_str()).collect()
}

#[tokio::test]
async fn restoring_an_unknown_stamp_or_page_fails_and_changes_nothing() {
    let dir = temp_dir("restore-unknown");
    let library = Library::load(dir.clone());
    let page = library.active().await;
    library.append(&page, ink("1")).await;

    let unknown_stamp = library.restore(&page, Some(123_456)).await.expect("runs");
    assert_eq!(None, unknown_stamp, "no archive at that stamp");
    assert_eq!(1, library.strokes(&page).await.len(), "page untouched");
    assert!(library.history(&page).expect("reads").is_empty());

    let unknown_page = library
        .restore("pg_doesnotexist000", Some(5))
        .await
        .expect("runs");
    assert_eq!(None, unknown_page, "no such page");
    assert!(library.content("pg_doesnotexist000").await.is_none());

    let reloaded = Library::load(dir.clone());
    assert_eq!(1, reloaded.strokes(&page).await.len(), "disk untouched");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_restore_archives_the_current_page_so_it_can_be_undone() {
    let dir = temp_dir("restore-undo");
    let library = Library::load(dir.clone());
    let page = library.active().await;
    library.append(&page, ink("1")).await;
    library.clear_page(&page).await;
    library.append(&page, ink("2")).await;

    library
        .restore(&page, None)
        .await
        .expect("runs")
        .expect("archive");
    assert_eq!(vec!["1"], seeds(&library.strokes(&page).await));
    assert_eq!(
        2,
        library.history(&page).expect("reads").len(),
        "now archived"
    );

    library
        .restore(&page, None)
        .await
        .expect("runs")
        .expect("archive");
    assert_eq!(
        vec!["2"],
        seeds(&library.strokes(&page).await),
        "the restore itself was undone"
    );
    let reloaded = Library::load(dir.clone());
    assert_eq!(vec!["2"], seeds(&reloaded.strokes(&page).await));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn replacing_with_a_repeated_seed_keeps_cache_and_database_equal() {
    let dir = temp_dir("replace-dedupe");
    let library = Library::load(dir.clone());
    let page = library.active().await;
    let total = library
        .replace_strokes(&page, &[], vec![ink("7"), ink("7")])
        .await;
    assert_eq!(1, total);
    assert_eq!(1, library.strokes(&page).await.len());
    let total = library
        .replace(&page, vec![ink("8"), ink("8"), ink("9")])
        .await;
    assert_eq!(2, total);
    assert_eq!(vec!["8", "9"], seeds(&library.strokes(&page).await));

    let reloaded = Library::load(dir.clone());
    assert_eq!(vec!["8", "9"], seeds(&reloaded.strokes(&page).await));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Makes every `DELETE FROM objects` fail, standing in for a full disk or an I/O error.
fn refuse_deletes(library: &Library) {
    library
        .store
        .conn
        .lock()
        .expect("lock")
        .execute_batch(
            "CREATE TRIGGER refuse_delete BEFORE DELETE ON objects
             BEGIN SELECT RAISE(ABORT, 'disk refused'); END;",
        )
        .expect("trigger");
}

#[tokio::test]
async fn a_refused_database_write_changes_nothing_and_tells_nobody() {
    let dir = temp_dir("db-refuses");
    let library = Library::load(dir.clone());
    let page = library.active().await;
    library.append(&page, ink("1")).await;
    library.append(&page, stroke("2", AGENT_LAYER)).await;
    refuse_deletes(&library);
    let mut frames = library.subscribe();

    assert_eq!(0, library.remove_strokes(&page, &["1".to_owned()]).await);
    assert_eq!(0, library.clear_agent(&page).await);
    assert_eq!(0, library.clear_page(&page).await);
    assert_eq!(
        0,
        library
            .replace_strokes(&page, &["1".to_owned()], vec![ink("3")])
            .await
    );
    assert_eq!(0, library.replace(&page, vec![ink("4")]).await);

    assert_eq!(vec!["1", "2"], seeds(&library.strokes(&page).await));
    assert!(
        frames.try_recv().is_err(),
        "no frame may announce an edit the database refused"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn an_oversized_or_out_of_range_stroke_is_bounded() {
    let dir = temp_dir("bounds");
    let library = Library::load(dir.clone());
    let page = library.active().await;
    let mut huge = ink("1");
    let count = MAX_STROKE_SAMPLES + 1;
    huge.samples = vec![0.0; count * 5];
    huge.times = vec![0; count];
    assert!(!library.append(&page, huge).await, "over the sample cap");

    let mut loud = ink("2");
    loud.scale = 1.0e9;
    assert!(library.append(&page, loud).await);
    let stored = library.strokes(&page).await;
    assert!(stored[0].scale <= 10.0, "scale clamped to the UI range");
    let _ = std::fs::remove_dir_all(&dir);
}
