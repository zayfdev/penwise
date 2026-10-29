//! `GET /marks` — the mark bundle (design-harness §7.2, C2): everything an agent needs to answer "what did I
//! mark?" without opening the page, built from the stored scene plus one headless page per (frame, state).
//!
//! Checkpoints are per agent name and are op-log seqs: a page's item rows carry `updated_seq`, so
//! `since=last` is "the mark's row changed after this agent's last call". The checkpoint advances on every call,
//! including calls that find nothing, exactly as the contract says. The bundle's own JSON and its crops live in
//! [`super::entry`] and [`super::images`].

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Query, State};
use penwise_core::item::{ItemId, Scene};
use penwise_core::marks::{FrameLayouts, MarkEntry, MarkStatus, MarkView};
use penwise_proto::PageId;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{ApiError, AppState, page_of};
use crate::storage::PageView;

use super::entry::{bundle, page_bundles};
use super::frame::FrameView;
use super::scene::{frame_item, frame_keys, frame_position};

/// `GET /marks` query. Every field is optional; `status` defaults to `new` + `reopened`, and `since=last` reads
/// the calling agent's checkpoint.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ListQuery {
    project: Option<String>,
    page: Option<String>,
    frame: Option<String>,
    status: Option<String>,
    since: Option<String>,
    agent: Option<String>,
}

/// What `since` asked for.
#[derive(Debug, Clone, Copy)]
enum Since {
    /// The agent's stored checkpoint.
    Last,
    /// An explicit op-log seq.
    After(u64),
}

/// The statuses a call lists: `all`, one name, or the default `new` + `reopened`.
fn wanted_statuses(text: Option<&str>) -> Result<Vec<MarkStatus>, ApiError> {
    match text {
        None | Some("" | "new+reopened") => Ok(vec![MarkStatus::New, MarkStatus::Reopened]),
        Some("all") => Ok(MarkStatus::ALL.to_vec()),
        Some(other) => MarkStatus::parse(other)
            .map(|status| vec![status])
            .ok_or_else(|| {
                ApiError::bad_request(
                    "status is one of new, seen, done, reopened, all, new+reopened",
                )
            }),
    }
}

fn since_filter(text: Option<&str>) -> Result<Option<Since>, ApiError> {
    match text {
        None | Some("" | "none") => Ok(None),
        Some("last") => Ok(Some(Since::Last)),
        Some(other) => other
            .parse::<u64>()
            .map(|seq| Some(Since::After(seq)))
            .map_err(|_| ApiError::bad_request("since is `last` or an op-log seq")),
    }
}

fn item_id(text: &str, what: &str) -> Result<ItemId, ApiError> {
    ItemId::from_hex(text)
        .ok_or_else(|| ApiError::bad_request(format!("{what} is 32 hex characters")))
}

/// The pages a call scans: one page, the page holding `frame`, or every live page.
async fn pages_to_scan(app: &AppState, query: &ListQuery) -> Result<Vec<PageId>, ApiError> {
    if let Some(frame) = &query.frame {
        let id = item_id(frame, "frame id")?;
        let page = app
            .storage
            .page_of_item(id)
            .await?
            .ok_or_else(|| ApiError::not_found("unknown frame"))?;
        return Ok(vec![page]);
    }
    if let Some(page) = &query.page {
        return Ok(vec![page_of(app, Some(page)).await?]);
    }
    let catalog = app.storage.catalog().await?;
    Ok(catalog
        .notebooks()
        .iter()
        .flat_map(|notebook| catalog.pages(notebook.id))
        .filter(|page| !page.deleted)
        .map(|page| page.id)
        .collect())
}

/// `GET /marks` — `{"checkpoint": <seq>, "marks": [bundle…]}`.
pub(crate) async fn list(
    State(app): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let agent = query
        .agent
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "agent".to_owned());
    let wanted = wanted_statuses(query.status.as_deref())?;
    let since = since_filter(query.since.as_deref())?;
    // The checkpoint is the log head read before anything else, so nothing that lands during this call is
    // skipped: at worst a mark shows up in two answers.
    let checkpoint = app.storage.status().await?.head_seq;
    let after = match since {
        None => None,
        Some(Since::After(seq)) => Some(seq),
        Some(Since::Last) => Some(
            app.storage
                .agent_checkpoint(agent.clone())
                .await?
                .unwrap_or(0),
        ),
    };
    let frame_filter = query
        .frame
        .as_deref()
        .map(|text| item_id(text, "frame id"))
        .transpose()?;
    let mut marks = Vec::new();
    for page in pages_to_scan(&app, &query).await? {
        let Some(view) = app.storage.page_view(page).await? else {
            continue;
        };
        if view.page.deleted {
            continue;
        }
        marks.extend(scan_page(&app, page, &view, &query, &wanted, after, frame_filter).await);
    }
    app.storage.set_agent_checkpoint(agent, checkpoint).await?;
    Ok(Json(json!({ "checkpoint": checkpoint, "marks": marks })))
}

/// The bundles of one page: marks grouped by (frame, state), because each state needs its own page.
async fn scan_page(
    app: &AppState,
    page: PageId,
    view: &PageView,
    query: &ListQuery,
    wanted: &[MarkStatus],
    after: Option<u64>,
    frame_filter: Option<ItemId>,
) -> Vec<Value> {
    let scene = view.replica.scene();
    let seqs: BTreeMap<ItemId, u64> = if after.is_some() {
        app.storage
            .item_seqs(page)
            .await
            .map(|rows| rows.into_iter().collect())
            .unwrap_or_default()
    } else {
        BTreeMap::new()
    };
    let groups = candidates(scene, &seqs, query, wanted, after, frame_filter);
    let mut out = Vec::new();
    for ((frame_id, state), entries) in groups {
        let Some(frame) = frame_item(scene, frame_id).cloned() else {
            continue;
        };
        let position = frame_position(scene, frame_id);
        let keys = frame_keys(scene, frame_id);
        match FrameView::open(
            app.inspect.as_ref(),
            &app.projects,
            frame_id,
            &frame,
            &keys,
            &state,
        )
        .await
        {
            Ok(browser) => {
                out.extend(
                    page_bundles(
                        app,
                        browser,
                        scene,
                        &frame,
                        position,
                        &entries,
                        view.updated_seq,
                    )
                    .await,
                );
            }
            Err(error) => {
                tracing::warn!(frame = %frame_id.to_hex(), %error, "no frame page; bundle without an element");
                for entry in &entries {
                    out.push(bundle(app, scene, entry, &frame, None).await);
                }
            }
        }
    }
    out
}

/// The marks a call wants, grouped by (frame, state): status, project, frame and `since` filters applied.
fn candidates(
    scene: &Scene,
    seqs: &BTreeMap<ItemId, u64>,
    query: &ListQuery,
    wanted: &[MarkStatus],
    after: Option<u64>,
    frame_filter: Option<ItemId>,
) -> BTreeMap<(ItemId, String), Vec<MarkEntry>> {
    let mut groups: BTreeMap<(ItemId, String), Vec<MarkEntry>> = BTreeMap::new();
    for entry in MarkView::new(scene, &FrameLayouts::new(), true).mark_entries() {
        let Some(frame) = frame_item(scene, entry.frame) else {
            continue;
        };
        if query
            .project
            .as_deref()
            .is_some_and(|id| id != frame.project)
            || frame_filter.is_some_and(|id| id != entry.frame)
            || !wanted.contains(&entry.status)
        {
            continue;
        }
        if let Some(after) = after
            && seqs.get(&entry.id).copied().unwrap_or(0) <= after
        {
            continue;
        }
        groups
            .entry((entry.frame, entry.state.clone()))
            .or_default()
            .push(entry);
    }
    groups
}
