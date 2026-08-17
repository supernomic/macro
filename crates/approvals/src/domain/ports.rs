//! Ports for approval gates.

use macro_uuid::Uuid;

use super::model::{ApprovalRequest, ApprovalStatus, ApprovalTransition, Result, ToolPolicy};

/// Filter for listing approval requests.
#[derive(Debug, Clone, Default)]
pub struct ApprovalFilter {
    /// Restrict to an organization.
    pub org_id: Option<i32>,
    /// Restrict to a status.
    pub status: Option<ApprovalStatus>,
    /// Restrict to items assigned to this user.
    pub assignee_user_id: Option<String>,
    /// Restrict to items on this team's queue.
    pub assignee_team_id: Option<Uuid>,
    /// Restrict to items with no direct assignee.
    pub unassigned_only: bool,
    /// Maximum rows.
    pub limit: i64,
}

/// Storage port for approval requests.
pub trait ApprovalRepo: Send + Sync + 'static {
    /// Insert a new request row.
    fn insert(&self, request: &ApprovalRequest) -> impl Future<Output = Result<()>> + Send;

    /// Fetch one request.
    fn get(&self, id: Uuid) -> impl Future<Output = Result<Option<ApprovalRequest>>> + Send;

    /// List requests matching a filter, newest first.
    fn list(
        &self,
        filter: &ApprovalFilter,
    ) -> impl Future<Output = Result<Vec<ApprovalRequest>>> + Send;

    /// The most recent request for a session + tool + arguments digest
    /// (any status). Used by the gate to resume across retries.
    fn find_latest_for_gate(
        &self,
        session_id: Option<Uuid>,
        tool_name: &str,
        arguments_digest: &str,
    ) -> impl Future<Output = Result<Option<ApprovalRequest>>> + Send;

    /// Atomically decide a pending request. Returns the updated row, or
    /// `None` when it was not pending.
    fn decide(
        &self,
        id: Uuid,
        approved: bool,
        decided_by: &str,
        note: Option<&str>,
    ) -> impl Future<Output = Result<Option<ApprovalRequest>>> + Send;

    /// Atomically consume a decided, unconsumed request. Returns the
    /// updated row, or `None` when it was already consumed or undecided.
    fn consume(&self, id: Uuid) -> impl Future<Output = Result<Option<ApprovalRequest>>> + Send;

    /// Reassign a pending request to a new user or team queue.
    fn reassign(
        &self,
        id: Uuid,
        to_user: Option<&str>,
        to_team: Option<Uuid>,
    ) -> impl Future<Output = Result<Option<ApprovalRequest>>> + Send;

    /// Cancel a pending request. Returns the updated row, or `None` when
    /// it was not pending.
    fn cancel(&self, id: Uuid) -> impl Future<Output = Result<Option<ApprovalRequest>>> + Send;

    /// Record an audited transition.
    fn insert_transition(
        &self,
        transition: &ApprovalTransition,
    ) -> impl Future<Output = Result<()>> + Send;

    /// List a request's transitions, oldest first.
    fn list_transitions(
        &self,
        approval_id: Uuid,
    ) -> impl Future<Output = Result<Vec<ApprovalTransition>>> + Send;
}

/// Storage port for org tool policies.
pub trait PolicyRepo: Send + Sync + 'static {
    /// List an org's policies.
    fn list_policies(
        &self,
        org_id: Option<i32>,
    ) -> impl Future<Output = Result<Vec<ToolPolicy>>> + Send;

    /// Insert or replace a policy (unique per org + agent + tool pattern).
    fn upsert_policy(&self, policy: &ToolPolicy) -> impl Future<Output = Result<()>> + Send;

    /// Delete a policy.
    fn delete_policy(&self, id: Uuid) -> impl Future<Output = Result<bool>> + Send;
}

/// Port over team membership (backed by MacroDB `team_user`).
pub trait TeamMembershipPort: Send + Sync + 'static {
    /// Whether a user belongs to a team.
    fn is_member(&self, user_id: &str, team_id: Uuid) -> impl Future<Output = Result<bool>> + Send;

    /// Member user ids of a team, in a stable order.
    fn team_members(&self, team_id: Uuid) -> impl Future<Output = Result<Vec<String>>> + Send;

    /// Team ids the user belongs to.
    fn user_teams(&self, user_id: &str) -> impl Future<Output = Result<Vec<Uuid>>> + Send;
}

/// Port for resuming the agent runtime when a request is decided.
pub trait ApprovalCallbackClient: Send + Sync + 'static {
    /// Deliver the decision to the runtime's callback URL. Failures are
    /// logged by the caller and do not roll back the decision.
    fn deliver(
        &self,
        callback_url: &str,
        payload: &serde_json::Value,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// Port for notifying humans about approval activity. A no-op
/// implementation is valid where notifications are not wired.
pub trait ApprovalNotifier: Send + Sync + 'static {
    /// Notify users that an approval request needs their attention.
    fn notify_assigned(
        &self,
        request: &ApprovalRequest,
        recipient_user_ids: &[String],
    ) -> impl Future<Output = Result<()>> + Send;
}
