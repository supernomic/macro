//! Domain models for skills.

use chrono::{DateTime, Utc};

/// A skill returned from a skill search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSummary {
    /// The document id of the skill.
    pub document_id: uuid::Uuid,
    /// The name of the skill.
    pub name: String,
    /// When the skill document was last updated, when known.
    pub updated_at: Option<DateTime<Utc>>,
}

/// How search terms are matched against skill names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkillMatchType {
    /// Prefix matching: a single-word term matches tokens that start with it.
    #[default]
    Partial,
    /// Whole-token / exact-phrase matching, no prefix expansion.
    Exact,
}

/// Who a skill is owned by. Personal skills may auto-apply; team and org
/// skills always go through proposal review before they become active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillScope {
    /// Owned by a single user.
    User,
    /// Owned by a team.
    Team,
    /// Owned by an organization.
    Org,
    /// Platform built-in (system skills).
    Platform,
}

impl SkillScope {
    /// Stable storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            SkillScope::User => "user",
            SkillScope::Team => "team",
            SkillScope::Org => "org",
            SkillScope::Platform => "platform",
        }
    }
}

/// Hermes-style trust tier recorded in skill provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustTier {
    /// Shipped with the product.
    Builtin,
    /// Reviewed and signed off.
    Verified,
    /// Community-sourced, not yet verified.
    Community,
    /// Unreviewed / agent-authored.
    Untrusted,
}

impl TrustTier {
    /// Stable storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            TrustTier::Builtin => "builtin",
            TrustTier::Verified => "verified",
            TrustTier::Community => "community",
            TrustTier::Untrusted => "untrusted",
        }
    }
}

/// OKF SPEC v0.2-inspired provenance frontmatter for a skill document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OkfFrontmatter {
    /// Catalog type (`skill`, `knowledge`, ...).
    pub kind: String,
    /// Source URIs or ids this content was derived from.
    pub sources: Vec<String>,
    /// Whether an agent generated this content.
    pub generated: bool,
    /// Whether a human has verified the content.
    pub verified: bool,
    /// Lifecycle status (`draft`, `active`, `deprecated`, `archived`).
    pub status: String,
    /// When the content should be considered stale.
    pub stale_after: Option<DateTime<Utc>>,
    /// SHA-256 hex of the body.
    pub content_hash: String,
}

/// Errors returned by skill operations.
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    /// The request is invalid.
    #[error("{0}")]
    InvalidRequest(String),
    /// The search backend failed.
    #[error("skill search failed")]
    SearchFailed(#[source] anyhow::Error),
    /// The listing backend failed.
    #[error("skill listing failed")]
    ListFailed(#[source] anyhow::Error),
}
