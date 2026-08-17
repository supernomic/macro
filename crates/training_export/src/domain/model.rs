//! Domain models for training export.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};

/// Result alias.
pub type Result<T> = std::result::Result<T, ExportError>;

/// Which projection of the ledger to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Projection {
    /// Post-compaction model-visible history.
    ModelHistory,
    /// Human transcript (user + assistant messages).
    HumanTranscript,
    /// Full training export (gated by sharing mode).
    TrainingExport,
}

impl Projection {
    /// Storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Projection::ModelHistory => "model_history",
            Projection::HumanTranscript => "human_transcript",
            Projection::TrainingExport => "training_export",
        }
    }

    /// Parse.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "model_history" => Some(Projection::ModelHistory),
            "human_transcript" => Some(Projection::HumanTranscript),
            "training_export" => Some(Projection::TrainingExport),
            _ => None,
        }
    }
}

/// Telemetry sharing consent applied to an export job (dsh).
///
/// Named [`SharingMode`] (not `FeedbackSharingMode`) so OpenAPI/utoipa emits a
/// distinct schema from the feedback sidecar's consent enum. utoipa's
/// `schema(rename = ...)` is a field rename, not a component-name alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SharingMode {
    /// Full trajectory.
    Full,
    /// Only `feedback/record` events.
    FeedbackOnly,
    /// Nothing exported.
    Disabled,
}

impl SharingMode {
    /// Storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            SharingMode::Full => "full",
            SharingMode::FeedbackOnly => "feedback_only",
            SharingMode::Disabled => "disabled",
        }
    }

    /// Parse.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "full" => Some(SharingMode::Full),
            "feedback_only" => Some(SharingMode::FeedbackOnly),
            "disabled" => Some(SharingMode::Disabled),
            _ => None,
        }
    }
}

/// An export job record.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExportJob {
    /// Job id.
    pub id: Uuid,
    /// Owning organization.
    pub org_id: Option<i32>,
    /// Projection.
    pub projection: Projection,
    /// Sharing mode applied.
    pub sharing_mode: SharingMode,
    /// Optional composition pin.
    pub composition_id: Option<String>,
    /// Window start.
    pub from_occurred_at: Option<DateTime<Utc>>,
    /// Window end.
    pub to_occurred_at: Option<DateTime<Utc>>,
    /// Status.
    pub status: String,
    /// Rows emitted.
    pub row_count: Option<i64>,
    /// Artifact URI when completed.
    pub artifact_uri: Option<String>,
    /// Error when failed.
    pub error: Option<String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Completed at.
    pub completed_at: Option<DateTime<Utc>>,
}

/// One projected event row.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProjectedEvent {
    /// Session.
    pub session_id: Uuid,
    /// Seq.
    pub seq: i64,
    /// Event type discriminant.
    pub event_type: String,
    /// Payload JSON.
    pub data: serde_json::Value,
    /// Composition id when known from a prior request/header.
    pub composition_id: Option<String>,
    /// Parent session when this row sits after a `session/seed` fork.
    pub parent_session_id: Option<Uuid>,
}

/// Export errors.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// Invalid request.
    #[error("{0}")]
    InvalidRequest(String),
    /// Not found.
    #[error("not found")]
    NotFound,
    /// Database.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
