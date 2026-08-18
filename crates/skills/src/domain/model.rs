//! Domain models for skills.

use chrono::{DateTime, Utc};
use std::fmt;
use std::str::FromStr;

#[cfg(test)]
mod test;

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
///
/// Shared vocabulary only — catalog/proposal storage lives in
/// `skill_governance`, not this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

    /// Parse from the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "user" => Some(SkillScope::User),
            "team" => Some(SkillScope::Team),
            "org" => Some(SkillScope::Org),
            "platform" => Some(SkillScope::Platform),
            _ => None,
        }
    }
}

impl fmt::Display for SkillScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SkillScope {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or("unknown skill scope")
    }
}

/// Hermes-style trust tier recorded in skill provenance.
///
/// Shared vocabulary only — provenance columns live in `skill_governance`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

    /// Parse from the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "builtin" => Some(TrustTier::Builtin),
            "verified" => Some(TrustTier::Verified),
            "community" => Some(TrustTier::Community),
            "untrusted" => Some(TrustTier::Untrusted),
            _ => None,
        }
    }
}

impl fmt::Display for TrustTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TrustTier {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or("unknown trust tier")
    }
}

/// Lifecycle status of a skill or knowledge document (OKF `status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OkfStatus {
    /// Not yet published.
    Draft,
    /// Live and injectable.
    Active,
    /// Still readable but should not be newly injected.
    Deprecated,
    /// Soft-deleted.
    Archived,
}

impl OkfStatus {
    /// Stable storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            OkfStatus::Draft => "draft",
            OkfStatus::Active => "active",
            OkfStatus::Deprecated => "deprecated",
            OkfStatus::Archived => "archived",
        }
    }

    /// Parse from the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(OkfStatus::Draft),
            "active" => Some(OkfStatus::Active),
            "deprecated" => Some(OkfStatus::Deprecated),
            "archived" => Some(OkfStatus::Archived),
            _ => None,
        }
    }
}

impl fmt::Display for OkfStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for OkfStatus {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or("unknown OKF status")
    }
}

/// OKF SPEC v0.2-inspired provenance frontmatter for a skill document.
///
/// This is the shared shape of OKF fields. Governed catalog rows and
/// proposal snapshots are stored by `skill_governance`, not here.
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
    pub status: OkfStatus,
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
