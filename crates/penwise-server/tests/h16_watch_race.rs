//! H-16: a browser switching pages (`Watch`) while another client creates a page must never miss that page
//! (review finding #9: a page snapshot taken at the head moved the client past catalog ops it was never sent).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]

mod common;

use std::time::Duration;

use common::{TestServer, converge};
use penwise_proto::{AnyOp, Background, ClientKind, OpId, PageChange, PageId, PageKind, PageOp};
use penwise_testkit::TestDevice;

#[tokio::test]
async fn a_page_created_while_a_browser_watches_another_page_is_never_missed() {
    let server = TestServer::start().await;
    let mut tablet = server.tablet(1);
    let mut browser = TestDevice::new(server.config(ClientKind::Browser, 0), 2);
    let nb = tablet.create_notebook("N");
    let first = tablet.create_page(nb, "first");
    let second = tablet.create_page(nb, "second");
    converge(&server, &mut [&mut tablet, &mut browser]).await;

    for round in 0..25_u32 {
        // The page is committed by the server device, so it arrives through the broadcast while the Watch
        // below is being served: either order must deliver it.
        let page = PageId::from_parts(server.now(), u128::from(round) + 1_000);
        let stamp = server.storage.server_stamp().await.expect("stamp");
        let op = AnyOp::Page(PageOp {
            op_id: OpId::from_parts(server.now(), u128::from(round) + 5_000),
            device: server.storage.server_device(),
            hlc: stamp,
            page,
            change: PageChange::Create {
                notebook: nb,
                title: format!("raced {round}"),
                position: 10 + i64::from(round),
                kind: PageKind::Infinite,
                background: Background::Blank,
            },
        });
        server
            .storage
            .append_ops(server.storage.server_device(), vec![op])
            .await
            .expect("page create");
        browser
            .watch(Some(if round % 2 == 0 { first } else { second }))
            .await
            .expect("watch");
        assert!(
            browser
                .pump_until(Duration::from_secs(5), |b| {
                    b.replica.catalog().pages(nb).iter().any(|p| p.id == page)
                })
                .await,
            "round {round}: the browser never learned about the new page"
        );
    }
    converge(&server, &mut [&mut tablet, &mut browser]).await;
    assert!(
        browser.events.apply_errors.is_empty(),
        "{:?}",
        browser.events.apply_errors
    );
}
