//! Penwise server library: storage (op log + materialized rows), sync sessions and the pieces the binary and
//! the in-process test harness share. The binary (`main.rs`) still carries the pre-rewrite routes until S4.

pub mod auth;
pub mod serve;
pub mod storage;
pub mod sync;
