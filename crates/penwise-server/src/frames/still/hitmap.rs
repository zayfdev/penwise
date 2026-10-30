//! The hit map: every visible element of the pre-rolled capture page, collected in one evaluation so that a
//! pen-down on a still is answered from memory, for the exact version the owner drew on.

use std::sync::Arc;

use crate::inspect::{Error, Inspect, Page};

use super::model::Hit;

const SCRIPT: &str = include_str!("hitmap.js");
/// The most elements reported. Pages beyond it lose the hits of their last elements in document order; the
/// browser path still answers when no still exists.
const MAX_HITS: u32 = 8_000;

/// Collects the hit map from `page` (at scroll 0, after the pre-roll).
///
/// # Errors
/// The page could not be evaluated or answered something that is not a hit list.
pub(super) async fn collect(inspect: &Inspect, page: &Page) -> Result<Arc<Vec<Hit>>, Error> {
    let value = inspect
        .eval(page, &format!("({SCRIPT})({MAX_HITS})"))
        .await?;
    let hits: Vec<Hit> =
        serde_json::from_value(value).map_err(|error| Error::Page(format!("hit map: {error}")))?;
    Ok(Arc::new(hits))
}
