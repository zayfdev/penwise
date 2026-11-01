//! Resolving a round (spec v2 §4): the round's frames freeze into history images, the live frames move one
//! column right, ghosts and variants clear, and the round's marks become `done`.
//!
//! The board edits are composed in [`crate::agent::freeze_round`] (it reuses the frame snapshot and the
//! variants-clear logic); this module owns the round's lifecycle around it: one resolve per project at a time,
//! Send before the freeze (so marks drawn while it runs belong to the next round), and the resolved row last.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use axum::http::StatusCode;
use serde_json::{Value, json};
use tokio::sync::OwnedMutexGuard;

use crate::api::{ApiError, AppState};
use crate::storage::{RoundOp, RoundStatus};

use super::Ledger;

/// One lock per project: two resolves of the same project never run their freeze together.
static LOCKS: LazyLock<Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

async fn lock(project: &str) -> OwnedMutexGuard<()> {
    let slot = {
        let mut locks = LOCKS.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(locks.entry(project.to_owned()).or_default())
    };
    slot.lock_owned().await
}

/// The latest sent round of `project` (what `round: true` means); 409 when none is waiting.
///
/// # Errors
/// 409 when no round has been sent and not resolved: an open round is never closed without its number.
pub(crate) async fn latest_sent(app: &AppState, project: &str) -> Result<u32, ApiError> {
    let ledger = Ledger::load(app, project).await?;
    ledger.latest_sent().map(|round| round.n).ok_or_else(|| {
        ApiError::new(
            StatusCode::CONFLICT,
            "no sent round to close; name the round to resolve (`round: n`, as `look` shows it)",
        )
    })
}

/// Resolves round `n` of `project` and describes what changed. Resolving a resolved round is a no-op that says
/// so; an open round with marks is sent first (its `sent_seq` fixes which marks it holds) and then resolved.
///
/// # Errors
/// 404 for a round that does not exist, 409 for an open round without marks or one sent by someone else
/// meanwhile, 503 when a frame cannot be snapshotted (nothing is changed).
pub(crate) async fn resolve_round(
    app: &AppState,
    project: &str,
    n: u32,
) -> Result<Value, ApiError> {
    let _one_at_a_time = lock(project).await;
    let mut ledger = Ledger::load(app, project).await?;
    if n != ledger.open_number() && ledger.round(n).is_none() {
        return Err(ApiError::not_found("unknown round"));
    }
    match ledger.status_of(n) {
        RoundStatus::Resolved => {
            return Ok(json!({ "n": n, "status": "resolved", "already": true }));
        }
        RoundStatus::Open => {
            if ledger.marks_of(n).is_empty() {
                return Err(ApiError::new(
                    StatusCode::CONFLICT,
                    "the open round has no marks to resolve",
                ));
            }
            app.storage
                .round_op(RoundOp::Send {
                    project: project.to_owned(),
                    n,
                })
                .await?
                .ok_or_else(|| {
                    ApiError::new(StatusCode::CONFLICT, format!("round {n} is no longer open"))
                })?;
            ledger = Ledger::load(app, project).await?;
        }
        RoundStatus::Sent => {}
    }
    let marks = ledger.marks_of(n);
    let newer: BTreeSet<_> = ledger
        .marks_after(n)
        .into_iter()
        .map(|mark| mark.entry.id)
        .collect();
    let frozen = crate::agent::freeze_round(app, project, n, &marks, &newer).await?;
    let round = app
        .storage
        .round_op(RoundOp::Resolve {
            project: project.to_owned(),
            n,
        })
        .await?
        .ok_or_else(|| ApiError::not_found("unknown round"))?;
    Ok(json!({
        "n": n,
        "status": round.status.as_str(),
        "marks_done": frozen.marks_done,
        "snapshots": frozen.snapshots,
        "already": frozen.already,
        "variants_removed": frozen.variants_removed,
    }))
}
