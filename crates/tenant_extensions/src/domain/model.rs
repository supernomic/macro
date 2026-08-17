//! Domain models for tenant extensions.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};

/// Result alias.
pub type Result<T> = std::result::Result<T, ExtensionError>;

/// Lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionStatus {
    /// Being authored.
    Draft,
    /// Awaiting inbox review.
    Proposed,
    /// Live.
    Active,
    /// Delisted / kill switch.
    Disabled,
    /// Rolled back to a prior snapshot.
    RolledBack,
}

impl ExtensionStatus {
    /// Storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            ExtensionStatus::Draft => "draft",
            ExtensionStatus::Proposed => "proposed",
            ExtensionStatus::Active => "active",
            ExtensionStatus::Disabled => "disabled",
            ExtensionStatus::RolledBack => "rolled_back",
        }
    }

    /// Parse.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(ExtensionStatus::Draft),
            "proposed" => Some(ExtensionStatus::Proposed),
            "active" => Some(ExtensionStatus::Active),
            "disabled" => Some(ExtensionStatus::Disabled),
            "rolled_back" => Some(ExtensionStatus::RolledBack),
            _ => None,
        }
    }
}

/// A tenant extension package.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TenantExtension {
    /// Extension id.
    pub id: Uuid,
    /// Owning organization.
    pub org_id: i32,
    /// Slug unique per org.
    pub slug: String,
    /// Display name.
    pub display_name: String,
    /// Package semver.
    pub version: String,
    /// Host SDK semver this artifact was stamped against.
    pub sdk_semver: String,
    /// marketplace.json-style manifest.
    pub manifest: serde_json::Value,
    /// SHA-256 of the artifact.
    pub artifact_hash: String,
    /// Status.
    pub status: ExtensionStatus,
    /// Token scopes granted to the extension principal.
    pub scopes: Vec<String>,
    /// Agent principal acting as this extension.
    pub principal_id: Option<Uuid>,
    /// When activated.
    pub activated_at: Option<DateTime<Utc>>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

/// Snapshot taken before activation / version bump.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExtensionSnapshot {
    /// Snapshot id.
    pub id: Uuid,
    /// Extension.
    pub extension_id: Uuid,
    /// Version captured.
    pub version: String,
    /// Manifest captured.
    pub manifest: serde_json::Value,
    /// Artifact hash captured.
    pub artifact_hash: String,
    /// Status captured.
    pub status: ExtensionStatus,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Who created it.
    pub created_by: String,
}

/// Fields to register or update an extension.
#[derive(Debug, Clone)]
pub struct RegisterExtension {
    /// Slug.
    pub slug: String,
    /// Display name.
    pub display_name: String,
    /// Version.
    pub version: String,
    /// SDK semver.
    pub sdk_semver: String,
    /// Manifest.
    pub manifest: serde_json::Value,
    /// Artifact hash.
    pub artifact_hash: String,
    /// Scopes.
    pub scopes: Vec<String>,
}

/// Extension errors.
#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    /// Invalid request.
    #[error("{0}")]
    InvalidRequest(String),
    /// Not found.
    #[error("not found")]
    NotFound,
    /// Re-tagged release: same version, different hash.
    #[error("refused re-tagged release: version {version} already has hash {existing}")]
    RetaggedRelease {
        /// Version.
        version: String,
        /// Existing hash.
        existing: String,
    },
    /// Invalid status transition.
    #[error("invalid status: {0}")]
    InvalidStatus(String),
    /// Database.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
