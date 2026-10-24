//! Owner: slice a (M2).
//!
//! Scene apply/invert, tombstones, bulk load, spatial index consistency, stroke families, hit testing,
//! lasso and selection actions, tested through the public API only.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    reason = "test harness: failures are reported by panicking"
)]

#[path = "m2_a/fixtures.rs"]
mod fixtures;
#[path = "m2_a/hit.rs"]
mod hit;
#[path = "m2_a/lasso.rs"]
mod lasso;
#[path = "m2_a/scene.rs"]
mod scene;
#[path = "m2_a/select.rs"]
mod select;
