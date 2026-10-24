//! Search across pages' recognised text: `GET /search?q=…`.
//!
//! Only pages that have been read by the handwriting model are searched; a page with ink but no text is simply
//! not a hit. Matching is case-insensitive on the cached transcription, newest-looking first by page order.

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};

use crate::AppState;
use crate::auth;

/// Longest query accepted; longer ones are unlikely to be real and cost a full scan.
const MAX_QUERY_CHARS: usize = 200;
/// Characters of context on each side of a match.
const SNIPPET_CONTEXT: usize = 60;
/// Most hits returned in one reply.
const MAX_HITS: usize = 50;

/// One page that contains the query.
#[derive(Debug, serde::Serialize)]
pub struct Hit {
    /// Page id.
    pub page: String,
    /// Page title.
    pub title: String,
    /// Matching text with a little context around the first match.
    pub snippet: String,
}

/// `GET /search?q=…` — pages whose recognised text contains `q`.
pub async fn search(
    State(app): State<AppState>,
    Query(query): Query<SearchQuery>,
    Query(auth_query): Query<crate::AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    let needle = query.q.unwrap_or_default().trim().to_owned();
    if needle.is_empty() {
        return axum::Json(serde_json::json!({ "query": "", "hits": [] })).into_response();
    }
    let needle: String = needle.chars().take(MAX_QUERY_CHARS).collect();
    let hits = collect(&app, &needle).await;
    axum::Json(serde_json::json!({ "query": needle, "hits": hits })).into_response()
}

async fn collect(app: &AppState, needle: &str) -> Vec<Hit> {
    let meta = app.library.meta().await;
    let mut hits = Vec::new();
    for page in meta.notebooks.iter().flat_map(|book| &book.pages) {
        if hits.len() >= MAX_HITS {
            break;
        }
        let Some(text) = app.library.text(&page.id).await else {
            continue;
        };
        if let Some(snippet) = snippet_of(&text, needle) {
            hits.push(Hit {
                page: page.id.clone(),
                title: page.title.clone(),
                snippet,
            });
        }
    }
    hits
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
pub struct SearchQuery {
    pub q: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_match_comes_back_with_context() {
        let text = "Shopping list\nmilk\neggs\nbread";
        let snippet = snippet_of(text, "EGGS").unwrap();
        assert!(snippet.contains("eggs"), "{snippet}");
        assert!(!snippet.contains('\n'), "collapsed to one line");
    }

    #[test]
    fn a_missing_needle_has_no_snippet() {
        assert!(snippet_of("milk and eggs", "butter").is_none());
        assert!(snippet_of("", "milk").is_none());
    }

    #[test]
    fn snippets_never_split_a_character() {
        let text = "héllo wörld with ümlauts and more ümlauts";
        let snippet = snippet_of(text, "wörld").unwrap();
        assert!(snippet.contains("wörld"));
    }

    #[test]
    fn a_match_near_the_edges_is_clamped() {
        let text = "start of a page with the word apples at the very beginning";
        assert!(snippet_of(text, "apples").unwrap().contains("apples"));
        let tail = "the end says bananas";
        assert!(snippet_of(tail, "bananas").unwrap().ends_with("bananas"));
    }
}
