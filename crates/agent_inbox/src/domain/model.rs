//! Unified inbox domain model.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};

/// Result alias for inbox operations.
pub type Result<T> = std::result::Result<T, InboxError>;

/// Discriminator for a unified inbox card. Locked JSON: `escalation` |
/// `approval` | `skill_proposal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InboxKind {
    /// An expert-handoff escalation.
    Escalation,
    /// A gated tool-call approval request.
    Approval,
    /// A staged skill create/patch/archive proposal.
    SkillProposal,
}

/// One card in `GET /inbox/mine`. Field names are locked for the UI agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InboxItem {
    /// `escalation` | `approval` | `skill_proposal`.
    pub kind: InboxKind,
    /// Source-row id.
    pub id: Uuid,
    /// Inbox-card title from the source record.
    pub title: String,
    /// Source status as its storage string.
    pub status: String,
    /// When the source row was created (RFC 3339).
    pub created_at: DateTime<Utc>,
    /// Existing DCS get path for this item (not a web route).
    pub href: String,
    /// The item is on the caller's personal assigned list.
    pub assigned_to_me: bool,
    /// The item is on a team queue the caller can claim/decide.
    pub team_queued: bool,
}

/// Personal unified inbox.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InboxView {
    /// Cards from all three sources, newest first.
    pub items: Vec<InboxItem>,
}

/// Errors from composing the three list sources.
#[derive(Debug, thiserror::Error)]
pub enum InboxError {
    /// Escalation list failed.
    #[error(transparent)]
    Escalation(#[from] escalations::domain::model::EscalationError),
    /// Approval list failed.
    #[error(transparent)]
    Approval(#[from] approvals::domain::model::ApprovalError),
    /// Skill-proposal list failed.
    #[error(transparent)]
    SkillProposal(#[from] skill_governance::domain::model::GovernanceError),
}
