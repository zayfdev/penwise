//! One error type for handlers: a status and a plain-text reason (the shape the old routes used).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::storage::{AppendError, RestoreError, StorageError};

/// A failed request.
#[derive(Debug)]
pub(crate) struct ApiError {
    pub(crate) status: StatusCode,
    pub(crate) message: String,
}

impl ApiError {
    pub(crate) fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, self.message).into_response()
    }
}

impl From<StorageError> for ApiError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::Closed => {
                Self::new(StatusCode::SERVICE_UNAVAILABLE, "server is stopping")
            }
            StorageError::BelowFloor { floor } => Self::new(
                StatusCode::GONE,
                format!("history starts after seq {floor}"),
            ),
            other => {
                tracing::error!(%other, "storage failure");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "storage failure")
            }
        }
    }
}

impl From<AppendError> for ApiError {
    fn from(error: AppendError) -> Self {
        match error {
            AppendError::Rejected(refused) => Self::bad_request(format!(
                "rejected: {}",
                refused
                    .iter()
                    .map(|r| r.reason.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            AppendError::Storage(error) => error.into(),
        }
    }
}

impl From<RestoreError> for ApiError {
    fn from(error: RestoreError) -> Self {
        match error {
            RestoreError::UnknownPage => Self::not_found("unknown page"),
            RestoreError::BelowFloor { floor } => Self::new(
                StatusCode::GONE,
                format!("history starts after seq {floor}"),
            ),
            RestoreError::Append(error) => error.into(),
            RestoreError::Storage(error) => error.into(),
        }
    }
}
