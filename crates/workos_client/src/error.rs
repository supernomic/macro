//! WorkOS client errors.

use crate::ids::WorkOsIdError;

/// Errors from the WorkOS API client.
#[derive(Debug, thiserror::Error)]
pub enum WorkOsClientError {
    /// WorkOS is not configured in this environment.
    #[error("WorkOS is not configured")]
    NotConfigured,
    /// A WorkOS identifier failed validation.
    #[error(transparent)]
    InvalidId(#[from] WorkOsIdError),
    /// The HTTP client failed to send the request.
    #[error("WorkOS request failed: {0}")]
    Transport(#[from] reqwest::Error),
    /// WorkOS returned a non-success status.
    #[error("WorkOS API error ({status}): {message}")]
    Api {
        /// HTTP status code from WorkOS.
        status: reqwest::StatusCode,
        /// Error message from the WorkOS response body.
        message: String,
    },
    /// Failed to serialize AuthKit state.
    #[error("unable to serialize WorkOS state")]
    InvalidState(#[from] serde_json::Error),
    /// Failed to build a WorkOS URL.
    #[error("invalid WorkOS URL: {0}")]
    InvalidUrl(#[from] url::ParseError),
}
