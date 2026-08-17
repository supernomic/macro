//! Domain models for lifecycle connectors.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};

/// Result alias.
pub type Result<T> = std::result::Result<T, ConnectorError>;

/// Supported lifecycle providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    /// Okta workforce identity.
    Okta,
    /// Iru / Kandji device management.
    Iru,
    /// Cisco Meraki network inventory.
    Meraki,
}

impl Provider {
    /// Stable storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Provider::Okta => "okta",
            Provider::Iru => "iru",
            Provider::Meraki => "meraki",
        }
    }

    /// Parse from storage.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "okta" => Some(Provider::Okta),
            "iru" => Some(Provider::Iru),
            "meraki" => Some(Provider::Meraki),
            _ => None,
        }
    }

    /// schema.org-inspired node type for a record kind.
    pub fn node_type(self, record_type: &str) -> &'static str {
        match (self, record_type) {
            (Provider::Okta, "user") => "Person",
            (Provider::Okta, "application") => "SoftwareApplication",
            (Provider::Iru, _) => "Device",
            (Provider::Meraki, _) => "Device",
            _ => "Thing",
        }
    }
}

/// A connected provider account.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ConnectorAccount {
    /// Account id.
    pub id: Uuid,
    /// Owning organization.
    pub org_id: Option<i32>,
    /// Provider.
    pub provider: Provider,
    /// Display name.
    pub display_name: String,
    /// Credential reference (never the secret).
    pub credential_ref: String,
    /// Last successful sync.
    pub last_synced_at: Option<DateTime<Utc>>,
    /// Opaque cursor.
    pub last_cursor: Option<String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// A synced external record.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ConnectorRecord {
    /// Record id.
    pub id: Uuid,
    /// Account.
    pub account_id: Uuid,
    /// Provider-stable id.
    pub external_id: String,
    /// Record kind (`user`, `device`, `application`).
    pub record_type: String,
    /// Raw payload.
    pub payload: serde_json::Value,
    /// Graph node this was projected onto.
    pub graph_node_id: Option<Uuid>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

/// One record to ingest.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct IngestRecord {
    /// Provider-stable id.
    pub external_id: String,
    /// Record kind.
    pub record_type: String,
    /// Display name for the graph node.
    pub display_name: String,
    /// Raw payload.
    pub payload: serde_json::Value,
}

/// Connector errors.
#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    /// Invalid request.
    #[error("{0}")]
    InvalidRequest(String),
    /// Not found.
    #[error("not found")]
    NotFound,
    /// Missing agent scope.
    #[error("missing required scope: {required}")]
    MissingScope {
        /// Required scope.
        required: String,
    },
    /// Graph ingest failed.
    #[error("graph ingest failed: {0}")]
    Graph(String),
    /// Database failure.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
