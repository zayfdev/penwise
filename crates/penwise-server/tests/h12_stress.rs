//! H-12: concurrent server-device writes (the HTTP annotate path appends through `Storage` as the server device),
//! `/sync` pushes and page deletes (8 tasks) -> DB == replica fold, every device converges, no FK errors.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::similar_names,
    clippy::items_after_statements
)]

mod common;

use std::time::Duration;

use common::{TestServer, converge, line};
use penwise_core::edit::Edit;
use penwise_core::item::Layer;
use penwise_proto::{AnyOp, Op, PageChange, PageId, PageOp, PagePatch};
use penwise_server::storage::AppendError;
use penwise_testkit::{IdGen, stroke_item};

async fn annotate(
    server_storage: penwise_server::storage::Storage,
    pages: Vec<PageId>,
    seed: u64,
) -> usize {
    let mut ids = IdGen::new(seed);
    let device = server_storage.server_device();
    let mut accepted = 0;
    for round in 0..15_u16 {
        let page = pages[usize::from(round) % pages.len()];
        let hlc = server_storage.server_stamp().await.expect("stamp");
        let item = stroke_item(
            ids.next_id_item(),
            65_536 * (i64::from(round) + 1),
            Layer::Agent,
            ids.seed(),
            &line(5.0, 5.0, 8),
        );
        let op = AnyOp::Item(Box::new(Op {
            op_id: ids.op(),
            device,
            hlc,
            page,
            edit: Edit::Add(item),
        }));
        match server_storage.append_ops(device, vec![op]).await {
            Ok(_) => accepted += 1,
            Err(AppendError::Rejected(_)) => {}
            Err(other) => panic!("storage error (FK?): {other:?}"),
        }
    }
    accepted
}

#[tokio::test]
async fn eight_concurrent_writers_leave_a_consistent_database() {
    let server = TestServer::start().await;
    let mut seed = server.tablet(100);
    let nb = seed.create_notebook("N");
    let pages: Vec<PageId> = (0..4)
        .map(|i| seed.create_page(nb, &format!("P{i}")))
        .collect();
    converge(&server, &mut [&mut seed]).await;

    let mut tasks = Vec::new();
    for n in 0..3_u64 {
        let pages = pages.clone();
        let mut dev = server.tablet(n + 1);
        tasks.push(tokio::spawn(async move {
            dev.online().await.expect("online");
            for i in 0..12_u16 {
                dev.draw(
                    pages[usize::from(i) % pages.len()],
                    &line(10.0 * f32::from(i), 10.0, 6),
                );
                dev.flush().await;
                dev.pump_until(Duration::from_secs(10), |d| d.outbox.is_empty())
                    .await;
            }
            dev
        }));
    }
    let mut annotators = Vec::new();
    for n in 0..3_u64 {
        annotators.push(tokio::spawn(annotate(
            server.storage.clone(),
            pages.clone(),
            500 + n,
        )));
    }
    let mut deleter = server.tablet(50);
    let pages2 = pages.clone();
    let deleters = tokio::spawn(async move {
        deleter.online().await.expect("online");
        for page in pages2.iter().take(2) {
            tokio::time::sleep(Duration::from_millis(30)).await;
            deleter.delete_page(*page);
            deleter.flush().await;
            deleter
                .pump_until(Duration::from_secs(10), |d| d.outbox.is_empty())
                .await;
        }
        let storage_page = PageOp {
            op_id: deleter.ids.op(),
            device: deleter.id,
            hlc: deleter.hlc.send(0),
            page: pages2[2],
            change: PageChange::Update(PagePatch {
                title: Some("renamed".into()),
                ..PagePatch::default()
            }),
        };
        let _ = deleter.push_op(&AnyOp::Page(storage_page));
        deleter.flush().await;
        deleter
            .pump_until(Duration::from_secs(10), |d| d.outbox.is_empty())
            .await;
        deleter
    });

    let mut devices = Vec::new();
    for t in tasks {
        devices.push(t.await.expect("writer"));
    }
    let mut accepted = 0;
    for t in annotators {
        accepted += t.await.expect("annotator");
    }
    devices.push(deleters.await.expect("deleter"));
    assert!(accepted > 0, "server-device appends must land");
    let mut refs: Vec<&mut penwise_testkit::TestDevice> = devices.iter_mut().collect();
    converge(&server, &mut refs).await;
}
