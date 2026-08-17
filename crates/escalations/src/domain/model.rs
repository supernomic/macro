//! Escalation domain model.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};

/// Result alias for escalation operations.
pub type Result<T> = std::result::Result<T, EscalationError>;

/// Urgency of an escalation; routing rules can gate on it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// No time pressure.
    Low,
    /// Default.
    Normal,
    /// Time-sensitive.
    High,
    /// Drop-everything.
    Urgent,
}

impl Priority {
    /// Stable storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Low => "low",
            Priority::Normal => "normal",
            Priority::High => "high",
            Priority::Urgent => "urgent",
        }
    }

    /// Parse from the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "low" => Some(Priority::Low),
            "normal" => Some(Priority::Normal),
            "high" => Some(Priority::High),
            "urgent" => Some(Priority::Urgent),
            _ => None,
        }
    }
}

/// Lifecycle of an escalation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EscalationStatus {
    /// Routed but unowned (sits in a team queue or unassigned).
    Open,
    /// A specific person owns it.
    Claimed,
    /// A human answered; the agent runtime has been (or is being) resumed.
    Resolved,
    /// Withdrawn without a resolution (e.g. requester solved it themselves).
    Cancelled,
}

impl EscalationStatus {
    /// Stable storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            EscalationStatus::Open => "open",
            EscalationStatus::Claimed => "claimed",
            EscalationStatus::Resolved => "resolved",
            EscalationStatus::Cancelled => "cancelled",
        }
    }

    /// Parse from the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "open" => Some(EscalationStatus::Open),
            "claimed" => Some(EscalationStatus::Claimed),
            "resolved" => Some(EscalationStatus::Resolved),
            "cancelled" => Some(EscalationStatus::Cancelled),
            _ => None,
        }
    }
}

/// One escalation: an agent's request for human-expert input.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Escalation {
    /// Escalation id (UUID v7).
    pub id: Uuid,
    /// Owning organization, when the creating principal is org-scoped.
    pub org_id: Option<i32>,
    /// Domain the escalation belongs to (e.g. `techops`).
    pub domain: String,
    /// Ledger session of the conversation that escalated, when any.
    pub session_id: Option<Uuid>,
    /// Macro user id of the person who originally asked, when known.
    pub requester_user_id: Option<String>,
    /// Human-readable requester (e.g. Slack handle) for display.
    pub requester_display: String,
    /// Source channel of the originating conversation (e.g. `slack`).
    pub source_channel: Option<String>,
    /// Short title for inbox cards.
    pub title: String,
    /// What the agent tried and where it got stuck (the expert's briefing).
    pub summary: String,
    /// Use-case tags for routing (e.g. `vpn`, `sso`).
    pub tags: Vec<String>,
    /// Urgency.
    pub priority: Priority,
    /// Lifecycle status.
    pub status: EscalationStatus,
    /// Person who owns it (set on claim / direct routing).
    pub assignee_user_id: Option<String>,
    /// Team whose queue holds it (set on team routing).
    pub assignee_team_id: Option<Uuid>,
    /// URL the runtime is called back on when the escalation resolves.
    pub callback_url: Option<String>,
    /// The expert's answer (set on resolve).
    pub resolution: Option<String>,
    /// Macro user id of the resolver.
    pub resolved_by: Option<String>,
    /// Creation instant.
    pub created_at: DateTime<Utc>,
    /// Claim instant, when claimed.
    pub claimed_at: Option<DateTime<Utc>>,
    /// Resolution instant, when resolved or cancelled.
    pub resolved_at: Option<DateTime<Utc>>,
}

/// Request to create an escalation (from the agent facade or internal).
#[derive(Debug, Clone)]
pub struct NewEscalation {
    /// Domain the escalation belongs to.
    pub domain: String,
    /// Ledger session of the escalating conversation.
    pub session_id: Option<Uuid>,
    /// Macro user id of the requester, when known.
    pub requester_user_id: Option<String>,
    /// Display name of the requester.
    pub requester_display: String,
    /// Source channel (e.g. `slack`).
    pub source_channel: Option<String>,
    /// Card title.
    pub title: String,
    /// The expert's briefing.
    pub summary: String,
    /// Routing tags.
    pub tags: Vec<String>,
    /// Urgency.
    pub priority: Priority,
    /// Resume callback URL.
    pub callback_url: Option<String>,
}

/// Where a routing rule sends matching escalations.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RouteTarget {
    /// Assign directly to one expert.
    User {
        /// The expert's Macro user id.
        user_id: String,
    },
    /// Put on a team queue for claiming.
    TeamQueue {
        /// The team.
        team_id: Uuid,
    },
    /// Assign round-robin across a team's available experts.
    TeamRoundRobin {
        /// The team.
        team_id: Uuid,
    },
}

/// A routing rule: conditions → target, evaluated in `position` order; the
/// first matching rule wins.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RoutingRule {
    /// Rule id.
    pub id: Uuid,
    /// Owning organization.
    pub org_id: Option<i32>,
    /// Domain this rule applies to.
    pub domain: String,
    /// Evaluation order (ascending; first match wins).
    pub position: i32,
    /// Tags condition: matches when empty or intersecting the escalation's
    /// tags.
    pub tags: Vec<String>,
    /// Source-channel condition: matches when absent or equal.
    pub source_channel: Option<String>,
    /// Minimum priority condition: matches when absent or `<=` the
    /// escalation's priority.
    pub min_priority: Option<Priority>,
    /// Where matching escalations go.
    pub target: RouteTarget,
}

impl RoutingRule {
    /// Whether this rule matches the given escalation attributes.
    pub fn matches(&self, request: &NewEscalation) -> bool {
        if self.domain != request.domain {
            return false;
        }
        if !self.tags.is_empty() && !self.tags.iter().any(|t| request.tags.contains(t)) {
            return false;
        }
        if let Some(channel) = &self.source_channel
            && request.source_channel.as_deref() != Some(channel.as_str())
        {
            return false;
        }
        if let Some(min) = self.min_priority
            && request.priority < min
        {
            return false;
        }
        true
    }
}

/// An expert's routing profile: which domains they cover and whether they
/// are currently taking work.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExpertProfile {
    /// The expert's Macro user id.
    pub user_id: String,
    /// Owning organization.
    pub org_id: Option<i32>,
    /// Domains the expert covers (empty = all).
    pub domains: Vec<String>,
    /// Expertise tags (informational; used by future rule refinement).
    pub tags: Vec<String>,
    /// Whether the expert is currently taking new escalations.
    pub available: bool,
}

/// An audited state transition on an escalation (claim, reassign, resolve).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EscalationTransition {
    /// Transition id.
    pub id: Uuid,
    /// The escalation.
    pub escalation_id: Uuid,
    /// Action name (`routed`, `claimed`, `reassigned`, `resolved`,
    /// `cancelled`).
    pub action: String,
    /// Who performed it (Macro user id, agent principal id, or `system`).
    pub actor_id: String,
    /// Assignee user before the transition.
    pub from_user_id: Option<String>,
    /// Assignee team before the transition.
    pub from_team_id: Option<Uuid>,
    /// Assignee user after the transition.
    pub to_user_id: Option<String>,
    /// Assignee team after the transition.
    pub to_team_id: Option<Uuid>,
    /// Free-form reason (required for reassign; feeds rule refinement).
    pub reason: Option<String>,
    /// Instant of the transition.
    pub created_at: DateTime<Utc>,
}

/// Errors for escalation operations.
#[derive(Debug, thiserror::Error)]
pub enum EscalationError {
    /// The escalation does not exist (or the caller cannot see it).
    #[error("escalation not found")]
    NotFound,
    /// The operation conflicts with the escalation's current status (e.g.
    /// claiming an already-claimed item).
    #[error("invalid status for this operation: {0}")]
    InvalidStatus(String),
    /// The caller is not allowed to perform this operation.
    #[error("forbidden: {0}")]
    Forbidden(String),
    /// The request is malformed.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// The agent token lacks a required scope.
    #[error("missing required scope: {required}")]
    MissingScope {
        /// The scope that was required.
        required: String,
    },
    /// Storage failure.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
