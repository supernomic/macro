//! Domain models for ticket mirrors.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};

/// Result alias.
pub type Result<T> = std::result::Result<T, MirrorError>;

/// External ticketing provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MirrorProvider {
    /// Zendesk.
    Zendesk,
    /// Jira.
    Jira,
}

impl MirrorProvider {
    /// Storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            MirrorProvider::Zendesk => "zendesk",
            MirrorProvider::Jira => "jira",
        }
    }

    /// Parse.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "zendesk" => Some(MirrorProvider::Zendesk),
            "jira" => Some(MirrorProvider::Jira),
            _ => None,
        }
    }
}

/// A live (or disconnected) mirror of a Macro entity onto an external ticket.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TicketMirror {
    /// Mirror id.
    pub id: Uuid,
    /// Owning organization.
    pub org_id: Option<i32>,
    /// Provider.
    pub provider: MirrorProvider,
    /// Macro entity type (source of truth).
    pub native_entity_type: String,
    /// Macro entity id.
    pub native_entity_id: String,
    /// External ticket id.
    pub foreign_id: String,
    /// External ticket URL (backlink).
    pub foreign_url: Option<String>,
    /// Summary mirrored outward.
    pub summary: String,
    /// External status snapshot.
    pub status: String,
    /// Last successful mirror.
    pub last_mirrored_at: DateTime<Utc>,
    /// When disconnected; native entity is unchanged.
    pub disconnected_at: Option<DateTime<Utc>>,
}

/// Fields to upsert a mirror.
#[derive(Debug, Clone)]
pub struct UpsertMirror {
    /// Provider.
    pub provider: MirrorProvider,
    /// Native entity type.
    pub native_entity_type: String,
    /// Native entity id.
    pub native_entity_id: String,
    /// External id.
    pub foreign_id: String,
    /// Backlink URL.
    pub foreign_url: Option<String>,
    /// Summary.
    pub summary: String,
    /// Status.
    pub status: String,
}

/// Mirror errors.
#[derive(Debug, thiserror::Error)]
pub enum MirrorError {
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
