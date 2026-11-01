//! A request body that is optional JSON. Android's `HttpURLConnection` sends an empty POST as
//! `application/x-www-form-urlencoded`, which axum's `Option<Json<_>>` rejects with 415 before the handler runs.
//! This extractor reads the bytes itself: an empty body is "no body" under any (or no) `Content-Type`, and a
//! non-empty body is JSON or a 400.

use axum::body::Bytes;
use axum::extract::{FromRequest, Request};
use serde::de::DeserializeOwned;

use super::ApiError;

/// `None` for an empty body; `Some` for a JSON body that parsed.
#[derive(Debug)]
pub(crate) struct OptionalJson<T>(pub Option<T>);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for OptionalJson<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|_| ApiError::bad_request("could not read the request body"))?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(Self(None));
        }
        serde_json::from_slice(&bytes)
            .map(|value| Self(Some(value)))
            .map_err(|_| ApiError::bad_request("the request body is not valid JSON"))
    }
}
