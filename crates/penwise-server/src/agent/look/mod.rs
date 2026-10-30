//! `GET /agent/look?project&frame?&mark?&images=overview|none&agent?` (spec v2 §5): the agent's one read.
//!
//! It answers with compact text (project, board, round, frames, one line per open mark, the owner's picks) and,
//! unless `images=none`, one overview image per frame the marks sit on. Which marks: the latest sent-but-not-
//! resolved round, else the open round. The marks it returns move `new → seen`, the agent's checkpoint
//! advances and the round's `looked_ms` is set (that is what silences `penwise inbox`). `mark=<id>` returns that
//! one mark's full bundle with its crops instead.

mod b64;
mod collect;
mod digits;
mod gather;
mod overview;
mod single;
#[cfg(test)]
mod tests;
mod text;

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::State;
use penwise_core::item::ItemId;
use penwise_core::marks::{MarkStatus, set_mark_status};
use penwise_proto::PageId;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, AppState, Author};
use crate::frames::registry::Project;
use crate::rounds::{DEFAULT_AGENT, Ledger, RoundMark};
use crate::storage::RoundOp;

use super::board::board_page;
use gather::{Views, bundles_by_mark, frame_lines, load_views, picks, rect_of};
use overview::{Numbered, overview};
use single::one_mark;
use text::{Header, MarkRow};

/// `POST /agent/look` body. Only `project` is required.
#[derive(Debug, Deserialize)]
pub(crate) struct LookQuery {
    project: String,
    #[serde(default)]
    frame: Option<String>,
    #[serde(default)]
    mark: Option<String>,
    #[serde(default)]
    images: Option<String>,
    #[serde(default)]
    agent: Option<String>,
}

/// `POST /agent/look` — `{"text", "images": [{"label", "png"}], "round", "marks": [ids]}`. A POST because it
/// has effects (marks become `seen`, the checkpoint and the round's `looked_ms` move).
pub(crate) async fn look(
    State(app): State<AppState>,
    Json(query): Json<LookQuery>,
) -> Result<Json<Value>, ApiError> {
    let project = app
        .projects
        .get(&query.project)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown project"))?;
    let want_overview = match query.images.as_deref() {
        None | Some("overview") => true,
        Some("none") => false,
        Some(_) => return Err(ApiError::bad_request("images is overview or none")),
    };
    let frame = query
        .frame
        .as_deref()
        .map(|text| {
            ItemId::from_hex(text)
                .ok_or_else(|| ApiError::bad_request("frame id is 32 hex characters"))
        })
        .transpose()?;
    let ledger = Ledger::load(&app, &project.id).await?;
    let n = ledger.current();
    let reply = match query.mark.as_deref() {
        Some(mark) => one_mark(&app, &ledger, mark).await?,
        None => round_view(&app, &project, &ledger, n, frame, want_overview).await?,
    };
    // A look at one frame or one mark is not a look at the round: it leaves the checkpoint and `looked_ms` alone.
    if query.frame.is_none() && query.mark.is_none() {
        record(&app, &ledger, &project.id, n, query.agent.as_deref()).await?;
    }
    Ok(Json(reply))
}

/// The checkpoint and the round's `looked_ms`.
async fn record(
    app: &AppState,
    ledger: &Ledger,
    project: &str,
    n: u32,
    agent: Option<&str>,
) -> Result<(), ApiError> {
    let head = app.storage.status().await?.head_seq;
    let agent = agent
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(DEFAULT_AGENT);
    app.storage
        .set_agent_checkpoint(agent.to_owned(), head)
        .await?;
    if ledger.round(n).is_none() {
        app.storage
            .round_op(RoundOp::EnsureOpen {
                project: project.to_owned(),
            })
            .await?;
    }
    app.storage
        .round_op(RoundOp::Looked {
            project: project.to_owned(),
            n,
        })
        .await?;
    Ok(())
}

fn row_of<'v>(row: &(&RoundMark, usize, &'v Value)) -> MarkRow<'v> {
    MarkRow {
        frame: row.1,
        bundle: row.2,
    }
}

/// The marks `look` lists: round `n`'s open ones plus every reopened mark of any round (the owner is asking
/// again), and, apart, the open marks drawn since `n` was sent on screens `n` touched.
fn marks_to_show(
    ledger: &Ledger,
    n: u32,
    frame: Option<ItemId>,
) -> (Vec<&RoundMark>, Vec<&RoundMark>) {
    let wanted = |mark: &&RoundMark| frame.is_none_or(|only| only == mark.entry.frame);
    let in_round = ledger.marks_of(n);
    let mut shown: Vec<&RoundMark> = in_round
        .iter()
        .copied()
        .filter(|mark| mark.entry.status != MarkStatus::Done)
        .filter(wanted)
        .collect();
    for mark in ledger.reopened() {
        if wanted(&mark) && !shown.iter().any(|known| known.entry.id == mark.entry.id) {
            shown.push(mark);
        }
    }
    // The same screens (route and preset) the round touched, variants of them included.
    let touched: Vec<(penwise_core::item::FramePreset, &str)> = in_round
        .iter()
        .map(|mark| (mark.preset, mark.route.as_str()))
        .collect();
    let since: Vec<&RoundMark> = ledger
        .marks_after(n)
        .into_iter()
        .filter(|mark| mark.entry.status != MarkStatus::Done)
        .filter(|mark| touched.contains(&(mark.preset, mark.route.as_str())))
        .filter(wanted)
        .filter(|mark| !shown.iter().any(|known| known.entry.id == mark.entry.id))
        .collect();
    (shown, since)
}

async fn round_view<'a>(
    app: &AppState,
    project: &Project,
    ledger: &'a Ledger,
    n: u32,
    frame: Option<ItemId>,
    want_overview: bool,
) -> Result<Value, ApiError> {
    let (shown, since) = marks_to_show(ledger, n, frame);
    let everything: Vec<&RoundMark> = shown.iter().chain(&since).copied().collect();
    let board = board_page(app, project).await?;
    let views = load_views(app, board, &everything).await?;
    let bundles = bundles_by_mark(app, &views, &everything).await;
    let (frames, tags) = frame_lines(&views, board, &everything);
    let rows_of = |marks: &[&'a RoundMark]| -> Vec<(&'a RoundMark, usize, &Value)> {
        let mut rows: Vec<(&RoundMark, usize, &Value)> = marks
            .iter()
            .filter_map(|mark| {
                let bundle = bundles.get(&mark.entry.id)?;
                Some((*mark, *tags.get(&mark.entry.frame)?, bundle))
            })
            .collect();
        rows.sort_by_key(|(_, tag, _)| *tag);
        rows
    };
    let rows = rows_of(&shown);
    let since_rows = rows_of(&since);
    let header = Header {
        project: format!("{} ({})", project.name, project.id),
        board: board.map(|page| page.to_string()),
        round: n,
        status: ledger.status_of(n).as_str(),
        total: ledger.marks_of(n).len(),
        open: rows.len(),
    };
    let picks = board
        .and_then(|page| views.get(&page))
        .map(|view| picks(view.replica.scene()))
        .unwrap_or_default();
    let listed: Vec<MarkRow<'_>> = rows.iter().map(row_of).collect();
    let after_send: Vec<MarkRow<'_>> = since_rows.iter().map(row_of).collect();
    let mut images = Vec::new();
    if want_overview {
        images = overviews(app, &views, &rows).await;
    }
    mark_seen(app, &views, &shown).await?;
    Ok(json!({
        "text": text::render(&header, &frames, &listed, &after_send, &picks),
        "images": images,
        "round": n,
        "marks": rows.iter().map(|(mark, _, _)| mark.entry.id.to_hex()).collect::<Vec<_>>(),
    }))
}

/// One overview image per frame that has located marks, numbered as in the text.
async fn overviews(
    app: &AppState,
    views: &Views,
    rows: &[(&RoundMark, usize, &Value)],
) -> Vec<Value> {
    let mut per_frame: BTreeMap<(usize, ItemId, PageId), Vec<Numbered>> = BTreeMap::new();
    for (index, (mark, tag, bundle)) in rows.iter().enumerate() {
        let Some(rect) = rect_of(bundle) else {
            continue;
        };
        per_frame
            .entry((*tag, mark.entry.frame, mark.page))
            .or_default()
            .push(Numbered {
                number: index + 1,
                mark: mark.entry.id,
                rect,
                strokes: mark.entry.strokes.clone(),
            });
    }
    let mut out = Vec::new();
    for ((tag, frame, page), marks) in per_frame {
        let Some(view) = views.get(&page) else {
            continue;
        };
        let label = format!(
            "overview f{} (marks {})",
            tag + 1,
            marks
                .iter()
                .map(|mark| mark.number.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        match overview(app, view.replica.scene(), frame, &marks).await {
            Ok(png) => out.push(json!({ "label": label, "png": b64::encode(&png) })),
            Err(error) => tracing::warn!(%error, "overview image unavailable"),
        }
    }
    out
}

/// Moves the shown marks that are still `new` to `seen`, one batch per page. A `reopened` mark stays reopened
/// until the agent resolves it.
async fn mark_seen(app: &AppState, views: &Views, shown: &[&RoundMark]) -> Result<(), ApiError> {
    for (page, view) in views {
        let mut author = Author::new(&app.storage);
        for mark in shown.iter().filter(|mark| mark.page == *page) {
            if mark.entry.status == MarkStatus::New {
                for edit in set_mark_status(view.replica.scene(), mark.entry.id, MarkStatus::Seen)
                    .map_err(|error| ApiError::bad_request(error.to_string()))?
                {
                    author.item(*page, edit).await?;
                }
            }
        }
        author.commit().await?;
    }
    Ok(())
}
