//! Domain models for the feedback sidecar.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};

/// Result alias.
pub type Result<T> = std::result::Result<T, FeedbackError>;

/// Editable thumbs rating stored in the sidecar (not the ledger).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RatingValue {
    /// Thumbs down.
    Down,
    /// Explicitly cleared.
    None,
    /// Thumbs up.
    Up,
}

impl RatingValue {
    /// Storage integer (`-1` / `0` / `1`).
    pub fn as_i16(self) -> i16 {
        match self {
            RatingValue::Down => -1,
            RatingValue::None => 0,
            RatingValue::Up => 1,
        }
    }

    /// Parse from storage.
    pub fn from_i16(v: i16) -> Option<Self> {
        match v {
            -1 => Some(RatingValue::Down),
            0 => Some(RatingValue::None),
            1 => Some(RatingValue::Up),
            _ => None,
        }
    }
}

/// Telemetry sharing consent for a session (dsh).
///
/// Named [`FeedbackSharingMode`] (not `SharingMode`) so OpenAPI/utoipa emits a
/// distinct schema from the training-export job's sharing-mode enum. utoipa's
/// `schema(rename = ...)` is a field rename, not a component-name alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackSharingMode {
    /// Full trajectory may be exported.
    Full,
    /// Only `feedback/record` events.
    FeedbackOnly,
    /// Nothing exported.
    Disabled,
}

impl FeedbackSharingMode {
    /// Storage string.
    pub fn as_str(self) -> &'static str {
        match self {
            FeedbackSharingMode::Full => "full",
            FeedbackSharingMode::FeedbackOnly => "feedback_only",
            FeedbackSharingMode::Disabled => "disabled",
        }
    }

    /// Parse from storage.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "full" => Some(FeedbackSharingMode::Full),
            "feedback_only" => Some(FeedbackSharingMode::FeedbackOnly),
            "disabled" => Some(FeedbackSharingMode::Disabled),
            _ => None,
        }
    }
}

/// One sidecar rating row.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MessageRating {
    /// Session.
    pub session_id: Uuid,
    /// Ledger seq of the rated event.
    pub target_seq: i64,
    /// Rating.
    pub rating: RatingValue,
    /// Optional note.
    pub note: Option<String>,
    /// Who rated (user id or agent principal id).
    pub rated_by: String,
    /// Last edit.
    pub updated_at: DateTime<Utc>,
}

/// Per-session training-export consent.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ConsentRecord {
    /// Session.
    pub session_id: Uuid,
    /// Organization.
    pub org_id: Option<i32>,
    /// Sharing mode.
    pub sharing_mode: FeedbackSharingMode,
    /// Who set it.
    pub set_by: String,
    /// Last edit.
    pub updated_at: DateTime<Utc>,
}

/// Feedback errors.
#[derive(Debug, thiserror::Error)]
pub enum FeedbackError {
    /// Invalid request.
    #[error("{0}")]
    InvalidRequest(String),
    /// Not found.
    #[error("not found")]
    NotFound,
    /// Missing capability.
    #[error("missing scope {required}")]
    MissingScope {
        /// Required scope.
        required: String,
    },
    /// Database.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
