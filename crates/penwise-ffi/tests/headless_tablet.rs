//! S5 done-when: a headless tablet (`SyncCore` + `SceneHandle`, bytes in/out) against the real server:
//! draw, erase, undo/redo, offline work, reconnect, restart → everything converges.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Tablet, TestServer, converge, line};
use penwise_core::item::LayerSet;
use penwise_ffi::sync_core::{NewPage, SyncCore};
use penwise_proto::{Background, PageKind};

/// Tablet A creates a notebook and a page, syncs, and holds `strokes` strokes.
async fn seeded(server: &TestServer, strokes: u16) -> (Tablet, penwise_proto::PageId) {
    let mut a = Tablet::new(server, 1);
    a.online().await;
    let notebook = a
        .core
        .create_notebook(None, "Notes", 0, common::now())
        .expect("notebook");
    let page = a
        .core
        .create_page(
            None,
            NewPage {
                notebook,
                title: "p1".to_owned(),
                position: 0,
                kind: PageKind::Fixed {
                    width: 1000.0,
                    height: 1000.0,
                },
                background: Background::Blank,
            },
            common::now(),
        )
        .expect("page");
    a.open_page(page);
    a.flush().await;
    for i in 0..strokes {
        a.draw(&line(50.0, 100.0 + f32::from(i) * 60.0, 30)).await;
    }
    a.settle().await;
    (a, page)
}

#[tokio::test]
async fn draw_syncs_to_a_second_tablet() {
    let server = TestServer::start().await;
    let (mut a, page) = seeded(&server, 3).await;
    let mut b = Tablet::new(&server, 2);
    b.online().await;
    b.open_page(page);
    assert_eq!(b.live(), 3);
    converge(&server, &mut [&mut a, &mut b]).await;
    server.shutdown().await;
}

#[tokio::test]
async fn erase_undo_redo_converge_live() {
    let server = TestServer::start().await;
    let (mut a, page) = seeded(&server, 2).await;
    let mut b = Tablet::new(&server, 2);
    b.online().await;
    b.open_page(page);
    // A precise erase through the first stroke splits it.
    let cut: Vec<[f32; 2]> = vec![[100.0, 60.0], [100.0, 140.0]];
    let edits = a.erase(&cut, 6.0, true).await;
    assert!(edits > 1, "a split emits adds and a delete, got {edits}");
    a.settle().await;
    converge(&server, &mut [&mut a, &mut b]).await;
    assert!(a.live() > 2, "pieces replace the original");
    assert_eq!(a.live(), b.live());
    // Undo restores the original stroke on both devices; redo cuts again.
    assert!(a.undo().await > 0);
    converge(&server, &mut [&mut a, &mut b]).await;
    assert_eq!(a.live(), 2);
    assert_eq!(b.live(), 2);
    assert!(a.redo().await > 0);
    converge(&server, &mut [&mut a, &mut b]).await;
    assert!(b.live() > 2);
    // Whole-stroke eraser.
    a.erase(&[[40.0, 160.0], [400.0, 160.0]], 10.0, false).await;
    converge(&server, &mut [&mut a, &mut b]).await;
    assert_eq!(a.live(), b.live());
    server.shutdown().await;
}

#[tokio::test]
async fn offline_work_on_both_devices_merges_after_reconnect() {
    let server = TestServer::start().await;
    let (mut a, page) = seeded(&server, 2).await;
    let mut b = Tablet::new(&server, 2);
    b.online().await;
    b.open_page(page);
    a.offline();
    b.offline();
    // A: draws and erases offline. B: draws and deletes offline.
    a.draw(&line(300.0, 300.0, 20)).await;
    a.erase(&[[100.0, 60.0], [100.0, 140.0]], 6.0, true).await;
    b.draw(&line(300.0, 500.0, 20)).await;
    b.erase(&[[40.0, 160.0], [400.0, 160.0]], 10.0, false).await;
    assert!(a.core.outbox().len() >= 2 && b.core.outbox().len() >= 2);
    converge(&server, &mut [&mut a, &mut b]).await;
    assert!(a.core.outbox().is_empty() && b.core.outbox().is_empty());
    assert_eq!(a.live(), b.live());
    server.shutdown().await;
}

#[tokio::test]
async fn undo_after_a_remote_change_skips_stale_fields() {
    let server = TestServer::start().await;
    let (mut a, page) = seeded(&server, 1).await;
    let mut b = Tablet::new(&server, 2);
    b.online().await;
    b.open_page(page);
    // B deletes the stroke A drew; A's undo of its Add must not resurrect anything wrong and still converge.
    let id = b.scene.as_ref().unwrap().render_order(LayerSet::ALL)[0];
    let edits = b.scene.as_mut().unwrap().delete_items(&[id]).unwrap();
    b.core.commit_edits(page, &edits, common::now()).unwrap();
    b.flush().await;
    b.settle().await;
    let head = server.head();
    a.catch_up_to(head).await;
    a.undo().await;
    converge(&server, &mut [&mut a, &mut b]).await;
    assert_eq!(a.live(), b.live());
    server.shutdown().await;
}

#[tokio::test]
async fn restart_with_persisted_state_resumes() {
    let server = TestServer::start().await;
    let (mut a, page) = seeded(&server, 2).await;
    a.offline();
    a.draw(&line(300.0, 300.0, 20)).await;
    let state = a.core.export_state().expect("export");
    let before = a.core.outbox().len();
    assert!(before > 0);
    drop(a);
    let restored = SyncCore::restore(&state, 99).expect("restore");
    assert_eq!(restored.outbox().len(), before);
    let mut a = Tablet::with_core(&server, restored);
    a.open_page(page);
    assert_eq!(a.live(), 3);
    converge(&server, &mut [&mut a]).await;
    assert!(a.core.outbox().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn fresh_install_same_device_replays_the_server_state() {
    let server = TestServer::start().await;
    let (a, page) = seeded(&server, 2).await;
    let device = a.core.device();
    let mut b = Tablet::with_core(&server, SyncCore::new(device, 5));
    b.online().await;
    b.open_page(page);
    assert_eq!(b.live(), 2);
    converge(&server, &mut [&mut b]).await;
    server.shutdown().await;
}

#[tokio::test]
async fn a_different_server_resets_the_replica_and_keeps_unsent_ops() {
    let mut server = TestServer::start().await;
    let (mut a, _page) = seeded(&server, 1).await;
    a.offline();
    a.draw(&line(300.0, 300.0, 20)).await;
    let pending = a.core.outbox().len();
    server.restart_fresh().await;
    a.online().await;
    assert_eq!(a.server_changes, 1, "identity change reported once");
    // The new server never had the page: the offline Add is refused, kept as a dead letter, never dropped.
    assert!(a.core.outbox().is_empty());
    assert_eq!(a.core.outbox().dead().len(), pending);
    server.shutdown().await;
}

/// A `PWST` page file: header, then big-endian stroke records like the Kotlin `SavedStrokeCodec`.
fn pwst(strokes: &[(i64, &[[f32; 2]])], torn: bool) -> Vec<u8> {
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
    if torn {
        out.extend([1, 0, 0]);
    }
    out
}

#[tokio::test]
async fn legacy_page_import_unions_by_deterministic_ids() {
    let server = TestServer::start().await;
    let page_text = "page-1";
    let (wavy, flat) = (line(20.0, 20.0, 12), line(20.0, 90.0, 12));
    let file = pwst(
        &[(-5_i64, wavy.as_slice()), (i64::MAX, flat.as_slice())],
        true,
    );
    let page_id = penwise_proto::legacy::legacy_page_id(page_text);

    let mut a = Tablet::new(&server, 1);
    a.online().await;
    let notebook = a
        .core
        .create_notebook(None, "Legacy", 0, common::now())
        .unwrap();
    let page = NewPage {
        notebook,
        title: "old".to_owned(),
        position: 0,
        kind: PageKind::Infinite,
        background: Background::Blank,
    };
    a.core
        .create_page(Some(page_id), page, common::now())
        .unwrap();
    let report = a
        .core
        .import_legacy_page(page_text, &file, common::now())
        .unwrap();
    assert_eq!(
        (report.added, report.already_present, report.skipped),
        (2, 0, 0)
    );
    a.settle().await;
    assert_eq!(server.fingerprint().live_on(page_id), 2);

    // A second import (same file) and a second device importing after catch-up add nothing.
    let again = a
        .core
        .import_legacy_page(page_text, &file, common::now())
        .unwrap();
    assert_eq!((again.added, again.already_present), (0, 2));
    let mut b = Tablet::new(&server, 2);
    b.online().await;
    let from_b = b
        .core
        .import_legacy_page(page_text, &file, common::now())
        .unwrap();
    assert_eq!((from_b.added, from_b.already_present), (0, 2));
    assert!(b.core.outbox().is_empty());
    converge(&server, &mut [&mut a, &mut b]).await;
    assert_eq!(server.fingerprint().live_on(page_id), 2);
    server.shutdown().await;
}
