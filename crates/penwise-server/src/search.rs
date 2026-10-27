//! Search across pages' recognised text: `GET /search?q=…`.
//!
//! Searches the live text items of every live page (recognized handwriting on the text layer, typed text):
//! a page with ink but no text item is simply not a hit. Matching is case-insensitive, hits come in catalog
//! order (notebooks, then pages by position).

use axum::Router;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::api::{ApiError, AppState};
use crate::textlayer;

/// Longest query accepted; longer ones are unlikely to be real and cost a full scan.
const MAX_QUERY_CHARS: usize = 200;
/// Characters of context on each side of a match.
const SNIPPET_CONTEXT: usize = 60;
/// Most hits returned in one reply.
const MAX_HITS: usize = 50;

/// One page that contains the query.
#[derive(Debug, serde::Serialize)]
struct Hit {
    /// Page id.
    pub page: String,
    /// Page title.
    pub title: String,
    /// Matching text with a little context around the first match.
    pub snippet: String,
}

/// `GET /search?q=…` — pages whose text items contain `q` (recognized handwriting and typed text alike).
async fn search(
    State(app): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Response, ApiError> {
    let needle = query.q.unwrap_or_default().trim().to_owned();
    if needle.is_empty() {
        return Ok(axum::Json(serde_json::json!({ "query": "", "hits": [] })).into_response());
    }
    let needle: String = needle.chars().take(MAX_QUERY_CHARS).collect();
    let hits = collect(&app, &needle).await?;
    Ok(axum::Json(serde_json::json!({ "query": needle, "hits": hits })).into_response())
}

async fn collect(app: &AppState, needle: &str) -> Result<Vec<Hit>, ApiError> {
    let catalog = app.storage.catalog().await?;
    let mut texts = textlayer::all_rows(&app.storage).await?;
    let mut hits = Vec::new();
    for notebook in catalog.notebooks() {
        for page in catalog.pages(notebook.id) {
            if hits.len() >= MAX_HITS {
                return Ok(hits);
            }
            let Some(rows) = texts.remove(&page.id) else {
                continue;
            };
            if let Some(snippet) = snippet_of(&textlayer::joined(&rows), needle) {
                hits.push(Hit {
                    page: page.id.to_string(),
                    title: page.title,
                    snippet,
                });
            }
        }
    }
    Ok(hits)
}

/// The first case-insensitive match in `text`, with context collapsed to one line.
fn snippet_of(text: &str, needle: &str) -> Option<String> {
    let haystack = text.to_lowercase();
    let pattern = needle.to_lowercase();
    let start = haystack.find(&pattern)?;
    // Map the byte offset back to a character boundary of the original text.
    let mut begin = start.saturating_sub(SNIPPET_CONTEXT);
    while !text.is_char_boundary(begin) {
        begin -= 1;
    }
    let mut end = (start + pattern.len() + SNIPPET_CONTEXT).min(text.len());
    while end > begin && !text.is_char_boundary(end) {
        end -= 1;
    }
    Some(text[begin..end].replace('\n', " ").trim().to_owned())
}

/// Query of the search route.
#[derive(Debug, serde::Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

/// `GET /search`.
pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/search", get(search))
}

#[cfg(test)]
mod tests;
