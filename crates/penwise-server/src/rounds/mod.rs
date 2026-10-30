//! Rounds (design-harness spec v2 §4): `POST /projects/<id>/rounds/send`, `GET /projects/<id>/rounds`,
//! `GET /projects/<id>/inbox` and `POST /projects/<id>/rounds/<n>/resolve`.
//!
//! A round is one request from the owner: the marks drawn since the last one was sent. The routes sit in the
//! `/projects` group (token, or a same-machine loopback request, the rule of `/pair/local`), so the tablet, the
//! PC window, `penwise inbox` and `penwise-mcp` all reach them the same way. Which marks a round holds comes
//! from the op-log seq that added them ([`ledger`]); resolving freezes the frame into history ([`resolve`]).

mod ledger;
mod resolve;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use penwise_core::marks::MarkStatus;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, AppState};
use crate::storage::{Round, RoundOp, RoundStatus};

pub(crate) use ledger::{Ledger, RoundMark};
pub(crate) use resolve::{latest_sent, resolve_round};

/// The round routes, merged into the project group by [`crate::api::projects::routes`].
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/projects/{id}/rounds", get(list))
        .route("/projects/{id}/rounds/send", post(send))
        .route("/projects/{id}/rounds/{n}/resolve", post(resolve))
        .route("/projects/{id}/inbox", get(inbox))
}

/// 404 unless `id` is a linked project.
pub(crate) async fn require_project(app: &AppState, id: &str) -> Result<(), ApiError> {
    app.projects
        .get(id)
        .await?
        .map(drop)
        .ok_or_else(|| ApiError::not_found("unknown project"))
}

/// `POST /projects/<id>/rounds/send {"n"?}` — closes the open round: `{"n", "marks"}`. `n` is the round the
/// caller saw open; it defaults to the current open round. 409 when that round is no longer open (someone
/// sent it first) or holds no marks.
async fn send(
    State(app): State<AppState>,
    Path(project): Path<String>,
    body: Option<Json<SendBody>>,
) -> Result<Json<Value>, ApiError> {
    require_project(&app, &project).await?;
    let ledger = Ledger::load(&app, &project).await?;
    let n = body
        .and_then(|Json(body)| body.n)
        .unwrap_or_else(|| ledger.open_number());
    if n != ledger.open_number() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            format!("round {n} is no longer open"),
        ));
    }
    let marks = ledger.marks_of(n).len();
    if marks == 0 {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "the open round has no marks to send",
        ));
    }
    // The compare-and-set runs in the storage writer: of two sends of the same round only one wins.
    let round = app
        .storage
        .round_op(RoundOp::Send { project, n })
        .await?
        .ok_or_else(|| {
            ApiError::new(StatusCode::CONFLICT, format!("round {n} is no longer open"))
        })?;
    Ok(Json(json!({ "n": round.n, "marks": marks })))
}

/// `POST .../rounds/send` body.
#[derive(Debug, Default, Deserialize)]
struct SendBody {
    #[serde(default)]
    n: Option<u32>,
}

/// `GET /projects/<id>/rounds` — every round with its status and mark counts, plus the open round that Send
/// would close (without storing it).
async fn list(
    State(app): State<AppState>,
    Path(project): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_project(&app, &project).await?;
    let ledger = Ledger::load(&app, &project).await?;
    let mut rounds: Vec<Value> = ledger
        .rounds()
        .iter()
        .map(|round| round_json(&ledger, round))
        .collect();
    let open = ledger.open_number();
    if ledger.round(open).is_none() {
        rounds.push(round_json(&ledger, &fresh(&project, open)));
    }
    Ok(Json(json!({ "rounds": rounds })))
}

/// A round that has no row yet.
fn fresh(project: &str, n: u32) -> Round {
    Round {
        project: project.to_owned(),
        n,
        status: RoundStatus::Open,
        sent_seq: None,
        resolved_seq: None,
        sent_ms: None,
        resolved_ms: None,
        looked_ms: None,
    }
}

fn round_json(ledger: &Ledger, round: &Round) -> Value {
    let marks = ledger.marks_of(round.n);
    let count = |status: MarkStatus| marks.iter().filter(|m| m.entry.status == status).count();
    json!({
        "n": round.n,
        "status": round.status.as_str(),
        "marks": marks.len(),
        "new": count(MarkStatus::New),
        "seen": count(MarkStatus::Seen),
        "done": count(MarkStatus::Done),
        "reopened": count(MarkStatus::Reopened),
        "sent_seq": round.sent_seq,
        "sent_ms": round.sent_ms,
        "resolved_ms": round.resolved_ms,
        "looked_ms": round.looked_ms,
    })
}

/// `GET /projects/<id>/inbox` — `{"line": "Penwise: round 3 sent — 4 marks on Desktop / (call look)"}`, or
/// `{"line": null}` when nothing waits for the agent. An unlinked project has nothing waiting.
async fn inbox(
    State(app): State<AppState>,
    Path(project): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let ledger = Ledger::load(&app, &project).await?;
    let checkpoint = app
        .storage
        .agent_checkpoint(DEFAULT_AGENT.to_owned())
        .await?
        .unwrap_or(0);
    Ok(Json(json!({ "line": inbox_line(&ledger, checkpoint) })))
}

/// The agent name `look` records its checkpoint under unless the call names another.
pub(crate) const DEFAULT_AGENT: &str = "agent";

/// The one line for what the agent has not looked at: the latest sent round since it was sent, and marks the
/// owner reopened since the agent's last look (`checkpoint`, an op-log seq).
fn inbox_line(ledger: &Ledger, checkpoint: u64) -> Option<String> {
    let round = ledger
        .latest_sent()
        .filter(|round| {
            round
                .sent_ms
                .is_some_and(|sent| round.looked_ms.is_none_or(|looked| looked < sent))
        })
        .map(|round| round.n);
    let in_round: Vec<&RoundMark> = round.map(|n| ledger.marks_of(n)).unwrap_or_default();
    let reopened: Vec<&RoundMark> = ledger
        .reopened()
        .into_iter()
        .filter(|mark| mark.updated_seq > checkpoint)
        .filter(|mark| !in_round.iter().any(|m| m.entry.id == mark.entry.id))
        .collect();
    if round.is_none() && reopened.is_empty() {
        return None;
    }
    let places: BTreeSet<String> = in_round
        .iter()
        .chain(&reopened)
        .map(|mark| format!("{} {}", capitalised(mark.preset.as_str()), mark.route))
        .collect();
    let on = places.into_iter().collect::<Vec<_>>().join(", ");
    let noun = |count: usize| if count == 1 { "mark" } else { "marks" };
    let reopened_note = (!reopened.is_empty()).then(|| format!("{} reopened", reopened.len()));
    Some(match round {
        Some(n) => {
            let extra = reopened_note.map_or_else(String::new, |note| format!(" (+{note})"));
            format!(
                "Penwise: round {n} sent \u{2014} {} {}{extra} on {on} (call look)",
                in_round.len(),
                noun(in_round.len()),
            )
        }
        None => format!(
            "Penwise: {} {} reopened on {on} (call look)",
            reopened.len(),
            noun(reopened.len())
        ),
    })
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// `POST /projects/<id>/rounds/<n>/resolve` — freezes the round into history; see [`resolve_round`].
async fn resolve(
    State(app): State<AppState>,
    Path((project, n)): Path<(String, u32)>,
) -> Result<Json<Value>, ApiError> {
    require_project(&app, &project).await?;
    Ok(Json(resolve_round(&app, &project, n).await?))
}
