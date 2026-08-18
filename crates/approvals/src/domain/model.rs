//! Approval-gate domain model.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Result alias for approval operations.
pub type Result<T> = std::result::Result<T, ApprovalError>;

/// The three policy outcomes for a gated tool call, ordered by
/// restrictiveness: combining policy layers takes the maximum, so lower
/// layers (the floor) can only be tightened, never loosened.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    /// Run without human involvement.
    Allow,
    /// Pause behind a pending approval request.
    RequireApproval,
    /// Refuse outright.
    Deny,
}

impl PolicyDecision {
    /// Stable storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            PolicyDecision::Allow => "allow",
            PolicyDecision::RequireApproval => "require_approval",
            PolicyDecision::Deny => "deny",
        }
    }

    /// Parse from the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "allow" => Some(PolicyDecision::Allow),
            "require_approval" => Some(PolicyDecision::RequireApproval),
            "deny" => Some(PolicyDecision::Deny),
            _ => None,
        }
    }
}

/// Whether `pattern` matches `value`. Patterns are exact strings, `*`
/// (everything), or a `prefix*` wildcard — the same grammar as agent token
/// scopes.
pub fn pattern_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    match pattern.strip_suffix('*') {
        Some(prefix) => value.starts_with(prefix),
        None => pattern == value,
    }
}

/// Specificity rank of a pattern: exact beats prefix-wildcard beats `*`.
fn pattern_specificity(pattern: &str) -> u8 {
    if pattern == "*" {
        0
    } else if pattern.ends_with('*') {
        1
    } else {
        2
    }
}

/// An org-configurable policy row: which decision applies to an agent +
/// tool pair, and who approves when the decision is `require_approval`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ToolPolicy {
    /// Policy id.
    pub id: Uuid,
    /// Owning organization.
    pub org_id: Option<i32>,
    /// Agent slug pattern (`techops`, `*`, `tech*`).
    pub agent_slug: String,
    /// Tool name pattern (`send_email`, `*`, `email.*`).
    pub tool_name: String,
    /// The decision this policy imposes.
    pub decision: PolicyDecision,
    /// Direct approver, when `require_approval`.
    pub approver_user_id: Option<String>,
    /// Approver team queue, when `require_approval`.
    pub approver_team_id: Option<Uuid>,
}

impl ToolPolicy {
    /// Whether this policy applies to the given agent + tool.
    pub fn matches(&self, agent_slug: &str, tool_name: &str) -> bool {
        pattern_matches(&self.agent_slug, agent_slug) && pattern_matches(&self.tool_name, tool_name)
    }

    /// Match specificity: more specific patterns win over broader ones.
    pub fn specificity(&self) -> u8 {
        pattern_specificity(&self.agent_slug) * 3 + pattern_specificity(&self.tool_name)
    }
}

/// One entry of the built-in policy floor.
#[derive(Debug, Clone)]
pub struct FloorEntry {
    /// Agent slug pattern.
    pub agent_slug: String,
    /// Tool name pattern.
    pub tool_name: String,
    /// The minimum decision for matching calls.
    pub decision: PolicyDecision,
}

/// The QM-style policy floor: baseline decisions org policies can only
/// tighten. An empty floor means everything defaults to `allow` unless an
/// org policy says otherwise.
#[derive(Debug, Clone, Default)]
pub struct PolicyFloor {
    /// Floor entries; the strictest matching entry applies.
    pub entries: Vec<FloorEntry>,
}

impl PolicyFloor {
    /// Built-in floor: outbound (`send_*`) and destructive (`delete_*`,
    /// `revoke_*`) tools require approval. Org policies can only tighten
    /// this (e.g. to `deny`), never loosen it to `allow`.
    pub fn builtin() -> Self {
        Self {
            entries: vec![
                FloorEntry {
                    agent_slug: "*".to_string(),
                    tool_name: "send_*".to_string(),
                    decision: PolicyDecision::RequireApproval,
                },
                FloorEntry {
                    agent_slug: "*".to_string(),
                    tool_name: "delete_*".to_string(),
                    decision: PolicyDecision::RequireApproval,
                },
                FloorEntry {
                    agent_slug: "*".to_string(),
                    tool_name: "revoke_*".to_string(),
                    decision: PolicyDecision::RequireApproval,
                },
            ],
        }
    }

    /// The strictest floor decision matching the agent + tool (defaults to
    /// `allow` when nothing matches).
    pub fn decision(&self, agent_slug: &str, tool_name: &str) -> PolicyDecision {
        self.entries
            .iter()
            .filter(|e| {
                pattern_matches(&e.agent_slug, agent_slug)
                    && pattern_matches(&e.tool_name, tool_name)
            })
            .map(|e| e.decision)
            .max()
            .unwrap_or(PolicyDecision::Allow)
    }
}

/// Lifecycle of an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    /// Waiting for a human decision.
    Pending,
    /// Approved; authorizes one retry of the gated call.
    Approved,
    /// Denied.
    Denied,
    /// Withdrawn by the agent or requester before a decision.
    Cancelled,
}

impl ApprovalStatus {
    /// Stable storage string.
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Denied => "denied",
            ApprovalStatus::Cancelled => "cancelled",
        }
    }

    /// Parse from the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(ApprovalStatus::Pending),
            "approved" => Some(ApprovalStatus::Approved),
            "denied" => Some(ApprovalStatus::Denied),
            "cancelled" => Some(ApprovalStatus::Cancelled),
            _ => None,
        }
    }
}

/// One approval request: a gated tool call waiting for (or decided by) a
/// human.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApprovalRequest {
    /// Request id (UUID v7).
    pub id: Uuid,
    /// Owning organization.
    pub org_id: Option<i32>,
    /// Slug of the agent principal that hit the gate.
    pub agent_slug: String,
    /// Ledger session of the gated conversation.
    pub session_id: Option<Uuid>,
    /// Macro user id of the person the agent is acting for, when known.
    pub requester_user_id: Option<String>,
    /// Human-readable requester for inbox cards.
    pub requester_display: String,
    /// The gated tool.
    pub tool_name: String,
    /// The proposed arguments, shown verbatim to the approver.
    pub arguments: serde_json::Value,
    /// SHA-256 hex digest of the canonical arguments.
    pub arguments_digest: String,
    /// The agent's explanation of what it wants to do and why.
    pub summary: String,
    /// Lifecycle status.
    pub status: ApprovalStatus,
    /// Direct approver, when routed to a person.
    pub assignee_user_id: Option<String>,
    /// Approver team queue, when routed to a team.
    pub assignee_team_id: Option<Uuid>,
    /// URL the runtime is called back on when decided.
    pub callback_url: Option<String>,
    /// Macro user id of the decider.
    pub decided_by: Option<String>,
    /// Optional note from the decider (shown to the agent and requester).
    pub decision_note: Option<String>,
    /// Creation instant.
    pub created_at: DateTime<Utc>,
    /// Decision instant.
    pub decided_at: Option<DateTime<Utc>>,
    /// When the decided request was consumed by a retry of the call.
    pub consumed_at: Option<DateTime<Utc>>,
}

/// A gate check for one proposed tool call.
#[derive(Debug, Clone)]
pub struct GateRequest {
    /// Slug of the calling agent.
    pub agent_slug: String,
    /// Ledger session of the conversation.
    pub session_id: Option<Uuid>,
    /// Macro user id of the person the agent acts for, when known.
    pub requester_user_id: Option<String>,
    /// Display name for inbox cards.
    pub requester_display: String,
    /// The tool about to run.
    pub tool_name: String,
    /// The proposed arguments.
    pub arguments: serde_json::Value,
    /// The agent's explanation for the approver.
    pub summary: String,
    /// URL the runtime is called back on when decided.
    pub callback_url: Option<String>,
}

/// Canonical SHA-256 hex digest of gate arguments.
pub fn arguments_digest(arguments: &serde_json::Value) -> String {
    let canonical = serde_json::to_string(arguments).unwrap_or_default();
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

/// Outcome of a gate check.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum GateOutcome {
    /// Proceed with the call (policy allows it, or a matching approval was
    /// consumed).
    Allow,
    /// Refuse the call.
    Deny {
        /// Why (policy, or the decider's note).
        reason: String,
    },
    /// The call is paused behind a pending approval request.
    Pending {
        /// The pending request (newly created or already open).
        request: Box<ApprovalRequest>,
    },
}

/// An audited state transition on an approval request.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApprovalTransition {
    /// Transition id.
    pub id: Uuid,
    /// The approval request.
    pub approval_id: Uuid,
    /// Action name (`routed`, `approved`, `denied`, `reassigned`,
    /// `cancelled`, `consumed`).
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
    /// Free-form reason (required for reassign).
    pub reason: Option<String>,
    /// Instant of the transition.
    pub created_at: DateTime<Utc>,
}

/// Errors for approval operations.
#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    /// The request does not exist (or the caller cannot see it).
    #[error("approval request not found")]
    NotFound,
    /// The operation conflicts with the request's status (e.g. deciding a
    /// non-pending item).
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
