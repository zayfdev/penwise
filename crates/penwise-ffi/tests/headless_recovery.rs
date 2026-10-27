//! Headless tablet against the real server: legacy import identities, import before catch-up, clock-skew
//! retry and the server-change decision.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

mod common;

use common::{Tablet, TestServer, converge, line, now};
use penwise_core::edit::Edit;
use penwise_core::item::{ItemId, Layer};
use penwise_ffi::sync_core::NewPage;
use penwise_proto::legacy::{legacy_item_id, legacy_notebook_id, legacy_page_id};
use penwise_proto::{Background, PageId, PageKind};
use penwise_testkit::stroke_item;

/// A `PWST` page file: header, then big-endian stroke records like the Kotlin `SavedStrokeCodec`.
fn pwst(strokes: &[(i64, &[[f32; 2]])]) -> Vec<u8> {
    let mut out = b"PWST".to_vec();
    out.push(1);
    for (seed, points) in strokes {
        out.push(1);
        out.extend(0_i32.to_be_bytes());
        out.extend((-16_777_216_i32).to_be_bytes());
        out.extend(1.0_f32.to_be_bytes());
        out.extend(seed.to_be_bytes());
        out.extend(i32::try_from(points.len()).unwrap().to_be_bytes());
        for (i, p) in points.iter().enumerate() {
            for v in [p[0], p[1], 0.5, 0.0, 0.0] {
                out.extend(v.to_be_bytes());
            }
            out.extend(i32::try_from(i * 8_000).unwrap().to_be_bytes());
        }
    }
    out
}

/// Creates the legacy notebook and page (deterministic ids) on `t`; returns the page.
fn legacy_page(t: &mut Tablet, page_text: &str) -> PageId {
    let notebook = t
        .core
        .create_notebook(Some(legacy_notebook_id("nb_1")), "Legacy", 0, now())
        .unwrap();
    let page = legacy_page_id(page_text);
    t.core
        .create_page(
            Some(page),
            NewPage {
                notebook,
                title: "old".to_owned(),
                position: 0,
                kind: PageKind::Infinite,
                background: Background::Blank,
            },
            now(),
        )
        .unwrap();
    page
}

#[tokio::test]
async fn strokes_sharing_a_seed_all_survive_the_import() {
    let server = TestServer::start().await;
    let lines: Vec<Vec<[f32; 2]>> = (0_u8..40)
        .map(|i| line(20.0, 20.0 + f32::from(i) * 30.0, 10))
        .collect();
    let mut strokes: Vec<(i64, &[[f32; 2]])> =
        lines.iter().map(|l| (0_i64, l.as_slice())).collect();
    strokes.push((77, lines[0].as_slice()));
    let file = pwst(&strokes);

    let mut a = Tablet::new(&server, 1);
    a.online().await;
    let page = legacy_page(&mut a, "page-1");
    let report = a.core.import_legacy_page("page-1", &file, now()).unwrap();
    assert_eq!(
        (
            report.added,
            report.already_present,
            report.skipped,
            report.seed_collisions
        ),
        (41, 0, 0, 39),
        "forty seed-0 strokes plus one distinct seed"
    );
    assert_eq!(a.core.item_ids(page).len(), 41);
    a.settle().await;
    assert_eq!(server.fingerprint().live_on(page), 41);

    // Repeat runs (and another device) derive the same identities: nothing is added twice.
    let again = a.core.import_legacy_page("page-1", &file, now()).unwrap();
    assert_eq!((again.added, again.already_present), (0, 41));
    let mut b = Tablet::new(&server, 2);
    b.online().await;
    let from_b = b.core.import_legacy_page("page-1", &file, now()).unwrap();
    assert_eq!((from_b.added, from_b.already_present), (0, 41));
    converge(&server, &mut [&mut a, &mut b]).await;
    assert_eq!(server.fingerprint().live_on(page), 41);
    // The first seed-0 stroke keeps the plain identity (what the server's own migration derives).
    assert!(
        a.core
            .replica()
            .page(page)
            .unwrap()
            .item(legacy_item_id("page-1", "0"))
            .is_some()
    );
    assert!(
        a.core
            .replica()
            .page(page)
            .unwrap()
            .item(legacy_item_id("page-1", "0#39"))
            .is_some()
    );
    server.shutdown().await;
}

#[tokio::test]
async fn an_import_made_before_catch_up_converges_to_the_server() {
    let server = TestServer::start().await;
    let (l0, l1, l2) = (
        line(20.0, 20.0, 12),
        line(20.0, 90.0, 12),
        line(20.0, 160.0, 12),
    );
    let file = pwst(&[
        (11, l0.as_slice()),
        (12, l1.as_slice()),
        (13, l2.as_slice()),
    ]);

    // The server's side of the story: another device migrated the rows, then a stroke was erased.
    let mut migrated = Tablet::new(&server, 1);
    migrated.online().await;
    let page = legacy_page(&mut migrated, "page-1");
    migrated
        .core
        .import_legacy_page("page-1", &file, now())
        .unwrap();
    migrated.settle().await;
    let erased = legacy_item_id("page-1", "12");
    migrated
        .core
        .commit_edits(page, &[Edit::Delete(erased)], now())
        .unwrap();
    migrated.settle().await;
    assert_eq!(server.fingerprint().live_on(page), 2);

    // The tablet starts the new app offline: it imports before it ever caught up.
    let mut a = Tablet::new(&server, 2);
    legacy_page(&mut a, "page-1");
    let report = a.core.import_legacy_page("page-1", &file, now()).unwrap();
    assert_eq!(report.added, 3);
    assert!(!a.core.is_caught_up(), "offline: not caught up");
    a.online().await;
    assert!(a.core.is_caught_up());
    converge(&server, &mut [&mut a, &mut migrated]).await;
    let replica = a.core.replica().page(page).unwrap();
    assert!(
        replica.item(erased).unwrap().deleted,
        "the erase survives on the tablet"
    );
    assert_eq!(server.fingerprint().live_on(page), 2);
    assert!(a.core.outbox().is_empty() && a.core.outbox().dead().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn work_stamped_by_a_fast_clock_is_restamped_not_dead_lettered() {
    let server = TestServer::start().await;
    let mut a = Tablet::new(&server, 1);
    let fast = now() + 600_000;
    let notebook = a.core.create_notebook(None, "n", 0, fast).unwrap();
    let page = a
        .core
        .create_page(
            None,
            NewPage {
                notebook,
                title: "p".to_owned(),
                position: 0,
                kind: PageKind::Infinite,
                background: Background::Blank,
            },
            fast,
        )
        .unwrap();
    let item = stroke_item(ItemId([5; 16]), 65_536, Layer::Ink, 3, &line(10.0, 10.0, 8));
    a.core.commit_edits(page, &[Edit::Add(item)], fast).unwrap();
    a.online().await;
    assert!(
        a.core.outbox().dead().is_empty(),
        "skew refusals are retried"
    );
    assert!(a.core.outbox().is_empty());
    assert_eq!(server.fingerprint().live_on(page), 1);
    // New work after the rebase is accepted too.
    a.open_page(page);
    a.draw(&line(50.0, 50.0, 8)).await;
    converge(&server, &mut [&mut a]).await;
    assert_eq!(server.fingerprint().live_on(page), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn keeping_the_ink_requeues_it_for_the_new_server() {
    let mut server = TestServer::start().await;
    let mut a = Tablet::new(&server, 1);
    a.online().await;
    let page = legacy_page(&mut a, "page-1");
    a.open_page(page);
    for i in 0_u8..3 {
        a.draw(&line(20.0, 20.0 + f32::from(i) * 40.0, 10)).await;
    }
    a.settle().await;
    server.restart_fresh().await;
    assert_eq!(server.head(), 0);
    a.on_server_change = None;
    a.online().await;
    assert_eq!(a.server_changes, 1);
    assert!(a.core.server_change_pending(), "held, not dropped");
    assert_eq!(a.core.item_ids(page).len(), 3);
    assert_eq!(server.head(), 0, "nothing was pushed to the new server");
    a.resolve(true);
    converge(&server, &mut [&mut a]).await;
    assert_eq!(server.fingerprint().live_on(page), 3);
    server.shutdown().await;
}
