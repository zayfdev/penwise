//! Addressing: page ids in URLs and query strings (ULID text, or a pre-rewrite `pg_…` alias) and the default
//! "current page".

use penwise_proto::{Catalog, NotebookId, PageId};

use super::{ApiError, AppState};
use crate::storage::AliasTarget;

/// The addressed page (ULID text or a legacy `pg_…` alias), or the server's current one when `requested` is
/// `None`.
///
/// # Errors
/// 400 for malformed ids, 404 for unknown or deleted pages (and for "no pages yet").
pub(crate) async fn page_of(app: &AppState, requested: Option<&str>) -> Result<PageId, ApiError> {
    let catalog = app.storage.catalog().await?;
    let page = match requested {
        Some(text) => parse_page(app, text).await?,
        None => current(app, &catalog).await?.ok_or_else(no_pages)?,
    };
    match catalog.page(page) {
        Some(found) if !found.deleted => Ok(page),
        _ => Err(ApiError::not_found("unknown page")),
    }
}

pub(crate) async fn parse_page(app: &AppState, text: &str) -> Result<PageId, ApiError> {
    if let Ok(id) = text.parse::<PageId>() {
        return Ok(id);
    }
    match app.storage.resolve_alias(text.to_owned()).await? {
        Some(AliasTarget::Page(id)) => Ok(id),
        Some(AliasTarget::Notebook(_)) => Err(ApiError::bad_request("that is a notebook id")),
        None if text.starts_with("pg_") => Err(ApiError::not_found("unknown page")),
        None => Err(ApiError::bad_request("malformed page id")),
    }
}

pub(crate) async fn parse_notebook(app: &AppState, text: &str) -> Result<NotebookId, ApiError> {
    if let Ok(id) = text.parse::<NotebookId>() {
        return Ok(id);
    }
    match app.storage.resolve_alias(text.to_owned()).await? {
        Some(AliasTarget::Notebook(id)) => Ok(id),
        Some(AliasTarget::Page(_)) => Err(ApiError::bad_request("that is a page id")),
        None if text.starts_with("nb_") => Err(ApiError::not_found("unknown notebook")),
        None => Err(ApiError::bad_request("malformed notebook id")),
    }
}

/// The current page when it is live, else the first live page (notebooks, then pages, in position order).
pub(crate) async fn current(app: &AppState, catalog: &Catalog) -> Result<Option<PageId>, ApiError> {
    if let Some(id) = app.storage.current_page().await?
        && catalog.page(id).is_some_and(|p| !p.deleted)
    {
        return Ok(Some(id));
    }
    Ok(first_live_page(catalog))
}

pub(crate) fn first_live_page(catalog: &Catalog) -> Option<PageId> {
    catalog
        .notebooks()
        .into_iter()
        .find_map(|nb| catalog.pages(nb.id).first().map(|p| p.id))
}

fn no_pages() -> ApiError {
    ApiError::not_found("no pages yet")
}
