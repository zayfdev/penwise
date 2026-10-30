//! `POST /agent/resolve` (spec v2 §5): the agent's verdicts on marks, and the end of the round.
//!
//! Statuses go through the same handler as `POST /marks/<id>/status` (synced, undoable edits). When the last
//! open mark of a sent round becomes `done` the round resolves by itself. `round: n` resolves round `n` now (the
//! number `look` shows); `round: true` means the latest sent round. An open round is never closed without its number.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::project_marks::{StatusBody, status};
use crate::api::{ApiError, AppState};
use crate::rounds::{Ledger, latest_sent, require_project, resolve_round};
use crate::storage::RoundStatus;
use penwise_core::marks::MarkStatus;

/// `{"project", "marks": [{id, status, note?}], "round"?}`.
#[derive(Debug, Deserialize)]
pub(crate) struct ResolveBody {
    project: String,
    #[serde(default)]
    marks: Vec<Verdict>,
    /// A round number, or `true` for the latest sent round.
    #[serde(default)]
    round: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct Verdict {
    id: String,
    status: String,
    /// Kept for the agent's own log; statuses carry no text.
    #[serde(default)]
    #[allow(
        dead_code,
        reason = "accepted so an agent's note is not a schema error"
    )]
    note: Option<String>,
}

/// Applies the verdicts, then resolves the round when asked or when nothing is left open in the sent round.
pub(crate) async fn resolve(
    State(app): State<AppState>,
    Json(body): Json<ResolveBody>,
) -> Result<Json<Value>, ApiError> {
    require_project(&app, &body.project).await?;
    let round = requested_round(&app, &body.project, body.round.as_ref()).await?;
    if body.marks.is_empty() && round.is_none() {
        return Err(ApiError::bad_request("give marks, or round: n"));
    }
    let mut applied = Vec::with_capacity(body.marks.len());
    for verdict in &body.marks {
        if !matches!(verdict.status.as_str(), "done" | "reopened" | "seen") {
            return Err(ApiError::bad_request(
                "status is one of done, reopened, seen",
            ));
        }
        let status_body: StatusBody = serde_json::from_value(json!({ "status": verdict.status }))
            .map_err(|error| ApiError::bad_request(error.to_string()))?;
        let Json(reply) = status(
            State(app.clone()),
            Path(verdict.id.clone()),
            Json(status_body),
        )
        .await?;
        applied.push(reply);
    }
    let round = match round {
        Some(n) => Some(resolve_round(&app, &body.project, n).await?),
        None => finished_round(&app, &body.project).await?,
    };
    Ok(Json(json!({ "marks": applied, "round": round })))
}

/// The round number the call names: a number is itself, `true` the latest sent round, `false`/absent none.
async fn requested_round(
    app: &AppState,
    project: &str,
    round: Option<&Value>,
) -> Result<Option<u32>, ApiError> {
    match round {
        None | Some(Value::Null | Value::Bool(false)) => Ok(None),
        Some(Value::Bool(true)) => latest_sent(app, project).await.map(Some),
        Some(value) => value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| ApiError::bad_request("round is a round number")),
    }
}

/// Resolves the sent round when every one of its marks is `done`.
async fn finished_round(app: &AppState, project: &str) -> Result<Option<Value>, ApiError> {
    let ledger = Ledger::load(app, project).await?;
    let Some(round) = ledger.latest_sent() else {
        return Ok(None);
    };
    let marks = ledger.marks_of(round.n);
    let all_done = !marks.is_empty()
        && marks
            .iter()
            .all(|mark| mark.entry.status == MarkStatus::Done);
    if round.status == RoundStatus::Sent && all_done {
        return Ok(Some(resolve_round(app, project, round.n).await?));
    }
    Ok(None)
}
