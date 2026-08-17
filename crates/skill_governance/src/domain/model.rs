//! Domain models for governed skills and staged proposals.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Result alias for governance operations.
pub type Result<T> = std::result::Result<T, GovernanceError>;

/// Who a skill is owned by. Personal skills may auto-apply; team and org
/// skills always go through proposal review before they become active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
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

    /// Team and org scopes always require a reviewed proposal.
    pub fn requires_review(&self) -> bool {
        matches!(
            self,
            SkillScope::Team | SkillScope::Org | SkillScope::Platform
        )
    }
}

/// Hermes-style trust tier recorded in skill provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
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

/// Lifecycle status of a skill or knowledge document (OKF `status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
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

/// SHA-256 hex of skill body text (OKF content hash).
pub fn content_hash(body: &str) -> String {
    hex::encode(Sha256::digest(body.as_bytes()))
}

/// A governed skill record.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkillRecord {
    /// Skill id.
    pub id: Uuid,
    /// Owning organization; `None` for platform skills.
    pub org_id: Option<i32>,
    /// Ownership scope.
    pub scope: SkillScope,
    /// Owner user when `scope` is `user`.
    pub owner_user_id: Option<String>,
    /// Owner team when `scope` is `team`.
    pub owner_team_id: Option<Uuid>,
    /// Stable slug (directory name / Flue skill name).
    pub slug: String,
    /// Display name.
    pub name: String,
    /// One-or-two sentence description (the catalog line the model always sees).
    pub description: String,
    /// Full SKILL.md body (instructions).
    pub body: String,
    /// Trust tier.
    pub trust_tier: TrustTier,
    /// OKF type (`skill`).
    pub okf_type: String,
    /// Source URIs or ids.
    pub okf_sources: Vec<String>,
    /// Whether an agent generated this content.
    pub okf_generated: bool,
    /// Whether a human has verified the content.
    pub okf_verified: bool,
    /// Lifecycle status.
    pub okf_status: OkfStatus,
    /// When the content should be considered stale.
    pub stale_after: Option<DateTime<Utc>>,
    /// SHA-256 hex of `body`.
    pub content_hash: String,
    /// Monotonic version; each approve/rollback bumps this.
    pub version: i32,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Last updated at.
    pub updated_at: DateTime<Utc>,
    /// Soft-delete timestamp.
    pub archived_at: Option<DateTime<Utc>>,
}

/// Catalog line served to Flue `useSkill` / `defineSkill`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkillCatalogEntry {
    /// Skill id.
    pub id: Uuid,
    /// Slug (Flue skill name).
    pub slug: String,
    /// Catalog description.
    pub description: String,
    /// Full instructions (loaded on activation).
    pub body: String,
    /// Content hash at serve time.
    pub version: String,
    /// Scope.
    pub scope: SkillScope,
    /// Trust tier.
    pub trust_tier: TrustTier,
}

impl From<&SkillRecord> for SkillCatalogEntry {
    fn from(skill: &SkillRecord) -> Self {
        Self {
            id: skill.id,
            slug: skill.slug.clone(),
            description: skill.description.clone(),
            body: skill.body.clone(),
            version: format!("{}:{}", skill.version, skill.content_hash),
            scope: skill.scope,
            trust_tier: skill.trust_tier,
        }
    }
}

/// A point-in-time snapshot of a skill, used for one-step rollback.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkillSnapshot {
    /// Snapshot id.
    pub id: Uuid,
    /// Skill this snapshot belongs to.
    pub skill_id: Uuid,
    /// Skill version captured.
    pub version: i32,
    /// Body at this version.
    pub body: String,
    /// Description at this version.
    pub description: String,
    /// Content hash at this version.
    pub content_hash: String,
    /// When the snapshot was taken.
    pub created_at: DateTime<Utc>,
    /// Who created it.
    pub created_by: String,
}

/// Kind of staged write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    /// Create a new skill.
    Create,
    /// Patch an existing skill.
    Patch,
    /// Archive an existing skill.
    Archive,
}

impl ProposalKind {
    /// Stable storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProposalKind::Create => "create",
            ProposalKind::Patch => "patch",
            ProposalKind::Archive => "archive",
        }
    }

    /// Parse from the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "create" => Some(ProposalKind::Create),
            "patch" => Some(ProposalKind::Patch),
            "archive" => Some(ProposalKind::Archive),
            _ => None,
        }
    }
}

/// Proposal review status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    /// Awaiting inbox review.
    Pending,
    /// Applied.
    Approved,
    /// Rejected; skill unchanged.
    Rejected,
    /// Previously approved, then rolled back to the snapshot.
    RolledBack,
}

impl ProposalStatus {
    /// Stable storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProposalStatus::Pending => "pending",
            ProposalStatus::Approved => "approved",
            ProposalStatus::Rejected => "rejected",
            ProposalStatus::RolledBack => "rolled_back",
        }
    }

    /// Parse from the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(ProposalStatus::Pending),
            "approved" => Some(ProposalStatus::Approved),
            "rejected" => Some(ProposalStatus::Rejected),
            "rolled_back" => Some(ProposalStatus::RolledBack),
            _ => None,
        }
    }
}

/// A staged skill write awaiting (or having received) inbox review.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkillProposal {
    /// Proposal id.
    pub id: Uuid,
    /// Owning organization.
    pub org_id: Option<i32>,
    /// Existing skill, when patching or archiving.
    pub skill_id: Option<Uuid>,
    /// Create / patch / archive.
    pub kind: ProposalKind,
    /// Target slug.
    pub slug: String,
    /// Target scope.
    pub target_scope: SkillScope,
    /// Owner user when targeting user scope.
    pub owner_user_id: Option<String>,
    /// Owner team when targeting team scope.
    pub owner_team_id: Option<Uuid>,
    /// Proposed name.
    pub proposed_name: String,
    /// Proposed catalog description.
    pub proposed_description: String,
    /// Proposed body.
    pub proposed_body: String,
    /// Human-readable diff summary (evidence-backed, prime-agent `/refine` shape).
    pub diff_summary: String,
    /// Trace excerpts / eval pointers backing the change.
    pub evidence: serde_json::Value,
    /// Proposing agent principal id, when an agent authored this.
    pub proposer_agent_id: Option<String>,
    /// Proposing user id, when a human authored this.
    pub proposer_user_id: Option<String>,
    /// Review status.
    pub status: ProposalStatus,
    /// Direct assignee.
    pub assignee_user_id: Option<String>,
    /// Team queue.
    pub assignee_team_id: Option<Uuid>,
    /// Snapshot taken at approve time (for rollback).
    pub snapshot_id: Option<Uuid>,
    /// Eval run that gated promotion, when any.
    pub eval_run_id: Option<String>,
    /// Whether that eval passed.
    pub eval_passed: Option<bool>,
    /// Who decided.
    pub decided_by: Option<String>,
    /// Decision note.
    pub decision_note: Option<String>,
    /// When decided.
    pub decided_at: Option<DateTime<Utc>>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Last updated at.
    pub updated_at: DateTime<Utc>,
}

/// Fields required to open a proposal.
#[derive(Debug, Clone)]
pub struct NewProposal {
    /// Create / patch / archive.
    pub kind: ProposalKind,
    /// Existing skill, required for patch/archive.
    pub skill_id: Option<Uuid>,
    /// Target slug.
    pub slug: String,
    /// Target scope.
    pub target_scope: SkillScope,
    /// Owner user when targeting user scope.
    pub owner_user_id: Option<String>,
    /// Owner team when targeting team scope.
    pub owner_team_id: Option<Uuid>,
    /// Proposed name.
    pub proposed_name: String,
    /// Proposed catalog description.
    pub proposed_description: String,
    /// Proposed body.
    pub proposed_body: String,
    /// Human-readable diff summary.
    pub diff_summary: String,
    /// Trace excerpts / eval pointers.
    pub evidence: serde_json::Value,
    /// Direct assignee, when known.
    pub assignee_user_id: Option<String>,
    /// Team queue, when known.
    pub assignee_team_id: Option<Uuid>,
}

/// An eval run that may gate org/team promotion.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SkillEvalRun {
    /// Run id.
    pub id: Uuid,
    /// Owning organization.
    pub org_id: Option<i32>,
    /// Skill under test, when it already exists.
    pub skill_id: Option<Uuid>,
    /// Proposal this run gates.
    pub proposal_id: Option<Uuid>,
    /// Pinned composition id.
    pub composition_id: String,
    /// Dataset / task set name.
    pub dataset: String,
    /// Whether the run passed the gate.
    pub passed: bool,
    /// Optional numeric score.
    pub score: Option<f64>,
    /// Full report blob.
    pub report: serde_json::Value,
    /// When the run was recorded.
    pub created_at: DateTime<Utc>,
}

/// A trace-refinement job result that emitted a proposal.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TraceRefinement {
    /// Job id.
    pub id: Uuid,
    /// Owning organization.
    pub org_id: Option<i32>,
    /// Proposal that was emitted.
    pub proposal_id: Option<Uuid>,
    /// Session the evidence was drawn from, when any.
    pub session_id: Option<Uuid>,
    /// Window start.
    pub window_start: DateTime<Utc>,
    /// Window end.
    pub window_end: DateTime<Utc>,
    /// Evidence excerpt stored with the job.
    pub evidence_excerpt: serde_json::Value,
    /// When the job ran.
    pub created_at: DateTime<Utc>,
}

/// Errors returned by governance operations.
#[derive(Debug, thiserror::Error)]
pub enum GovernanceError {
    /// The request is invalid.
    #[error("{0}")]
    InvalidRequest(String),
    /// The skill or proposal was not found (or is outside the caller's tenancy).
    #[error("not found")]
    NotFound,
    /// The proposal is not in a state that accepts this transition.
    #[error("invalid status: {0}")]
    InvalidStatus(String),
    /// Promotion is blocked until evals pass.
    #[error("eval gate failed: {0}")]
    EvalGate(String),
    /// The caller lacks a required agent token scope.
    #[error("missing required scope: {required}")]
    MissingScope {
        /// The scope that was required.
        required: String,
    },
    /// The caller is not allowed to perform this action.
    #[error("forbidden: {0}")]
    Forbidden(String),
    /// The storage backend failed.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
