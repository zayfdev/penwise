//! Clock calibration (design §2.2, D9): a tablet whose clock is far ahead of or behind the server's stays usable,
//! because `Welcome.server_now_ms` calibrates the stamps it creates. Without it every op would be rejected `ClockSkew`.
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

use common::{TestServer, converge, line};
use penwise_proto::ClientKind;
use penwise_testkit::TestDevice;

const MIN_MS: i64 = 60_000;

#[tokio::test]
async fn a_device_ten_minutes_ahead_is_calibrated_and_accepted() {
    let server = TestServer::start().await;
    let mut fast = TestDevice::new(server.config(ClientKind::Tablet, 10 * MIN_MS), 1);
    fast.online().await.expect("online");
    assert!(
        (fast.offset.millis() + 10 * MIN_MS).abs() < 2_000,
        "offset ~ -10 min, got {}",
        fast.offset.millis()
    );
    let nb = fast.create_notebook("N");
    let page = fast.create_page(nb, "P");
    fast.draw(page, &line(10.0, 10.0, 10));
    converge(&server, &mut [&mut fast]).await;
    assert!(
        fast.events.rejected.is_empty(),
        "calibrated stamps are within the skew window: {:?}",
        fast.events.rejected
    );
    assert_eq!(server.head(), 3);
}

#[tokio::test]
async fn an_uncalibrated_future_stamp_is_rejected_clock_skew() {
    let server = TestServer::start().await;
    let mut fast = TestDevice::new(server.config(ClientKind::Tablet, 10 * MIN_MS), 1);
    // Author before ever connecting: the offset is still zero, so stamps are 10 minutes in the future.
    let nb = fast.create_notebook("N");
    fast.online().await.expect("online and pushed");
    assert!(
        fast.events
            .rejected
            .iter()
            .any(|r| r.reason == penwise_proto::OpRejection::ClockSkew),
        "the server backstop rejects stamps beyond MAX_SKEW_MS: {:?}",
        fast.events.rejected
    );
    assert!(
        fast.outbox
            .dead()
            .iter()
            .any(|d| d.reason == penwise_proto::OpRejection::ClockSkew),
        "kept as dead letter"
    );
    let _ = nb;
    assert_eq!(server.head(), 0);
}

#[tokio::test]
async fn a_device_a_day_behind_is_accepted() {
    let server = TestServer::start().await;
    let mut slow = TestDevice::new(server.config(ClientKind::Tablet, -24 * 60 * MIN_MS), 2);
    slow.online().await.expect("online");
    let nb = slow.create_notebook("N");
    let page = slow.create_page(nb, "P");
    slow.draw(page, &line(10.0, 10.0, 10));
    converge(&server, &mut [&mut slow]).await;
    assert_eq!(server.head(), 3);
}
