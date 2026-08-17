//! Ports for escalations.

use macro_uuid::Uuid;

use super::model::{
    Escalation, EscalationStatus, EscalationTransition, ExpertProfile, Result, RoutingRule,
};

/// Filter for listing escalations.
#[derive(Debug, Clone, Default)]
pub struct EscalationFilter {
    /// Restrict to an organization.
    pub org_id: Option<i32>,
    /// Restrict to a status.
    pub status: Option<EscalationStatus>,
    /// Restrict to a domain.
    pub domain: Option<String>,
    /// Restrict to items assigned to this user.
    pub assignee_user_id: Option<String>,
    /// Restrict to items on this team's queue.
    pub assignee_team_id: Option<Uuid>,
    /// Restrict to unclaimed items (`status = open` and no assignee user).
    pub unclaimed_only: bool,
    /// Maximum rows.
    pub limit: i64,
}

/// Storage port for escalations.
pub trait EscalationRepo: Send + Sync + 'static {
    /// Insert a new escalation row.
    fn insert(&self, escalation: &Escalation) -> impl Future<Output = Result<()>> + Send;

    /// Fetch one escalation.
    fn get(&self, id: Uuid) -> impl Future<Output = Result<Option<Escalation>>> + Send;

    /// List escalations matching a filter, newest first.
    fn list(
        &self,
        filter: &EscalationFilter,
    ) -> impl Future<Output = Result<Vec<Escalation>>> + Send;

    /// Atomically claim an open escalation for a user. Returns the updated
    /// row, or `None` when the item was not open (someone else claimed it
    /// first).
    fn claim(
        &self,
        id: Uuid,
        user_id: &str,
    ) -> impl Future<Output = Result<Option<Escalation>>> + Send;

    /// Reassign to a new user or team queue, resetting to `open` when the
    /// target is a team.
    fn reassign(
        &self,
        id: Uuid,
        to_user: Option<&str>,
        to_team: Option<Uuid>,
    ) -> impl Future<Output = Result<Option<Escalation>>> + Send;

    /// Mark resolved with a resolution. Returns the updated row, or `None`
    /// when the item was already terminal.
    fn resolve(
        &self,
        id: Uuid,
        resolution: &str,
        resolved_by: &str,
    ) -> impl Future<Output = Result<Option<Escalation>>> + Send;

    /// Mark cancelled. Returns the updated row, or `None` when already
    /// terminal.
    fn cancel(&self, id: Uuid) -> impl Future<Output = Result<Option<Escalation>>> + Send;

    /// Record an audited transition.
    fn insert_transition(
        &self,
        transition: &EscalationTransition,
    ) -> impl Future<Output = Result<()>> + Send;

    /// List an escalation's transitions, oldest first.
    fn list_transitions(
        &self,
        escalation_id: Uuid,
    ) -> impl Future<Output = Result<Vec<EscalationTransition>>> + Send;
}

/// Storage port for routing configuration.
pub trait RoutingRepo: Send + Sync + 'static {
    /// List rules for an org + domain in evaluation order.
    fn list_rules(
        &self,
        org_id: Option<i32>,
        domain: &str,
    ) -> impl Future<Output = Result<Vec<RoutingRule>>> + Send;

    /// List every rule for an org (admin view).
    fn list_all_rules(
        &self,
        org_id: Option<i32>,
    ) -> impl Future<Output = Result<Vec<RoutingRule>>> + Send;

    /// Insert or replace a rule.
    fn upsert_rule(&self, rule: &RoutingRule) -> impl Future<Output = Result<()>> + Send;

    /// Delete a rule.
    fn delete_rule(&self, id: Uuid) -> impl Future<Output = Result<bool>> + Send;

    /// Fetch one expert profile.
    fn get_expert(
        &self,
        org_id: Option<i32>,
        user_id: &str,
    ) -> impl Future<Output = Result<Option<ExpertProfile>>> + Send;

    /// List expert profiles for an org.
    fn list_experts(
        &self,
        org_id: Option<i32>,
    ) -> impl Future<Output = Result<Vec<ExpertProfile>>> + Send;

    /// Insert or replace an expert profile.
    fn upsert_expert(&self, profile: &ExpertProfile) -> impl Future<Output = Result<()>> + Send;

    /// Read the round-robin pointer for a rule (last assigned user).
    fn round_robin_last(
        &self,
        rule_id: Uuid,
    ) -> impl Future<Output = Result<Option<String>>> + Send;

    /// Advance the round-robin pointer for a rule.
    fn set_round_robin_last(
        &self,
        rule_id: Uuid,
        user_id: &str,
    ) -> impl Future<Output = Result<()>> + Send;
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

/// Port for resuming the agent runtime when an escalation resolves.
pub trait EscalationCallbackClient: Send + Sync + 'static {
    /// Deliver the resolution to the runtime's callback URL. Failures are
    /// logged by the caller and do not roll back the resolution.
    fn deliver(
        &self,
        callback_url: &str,
        payload: &serde_json::Value,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// Port for notifying humans about escalation activity. Implementations
/// fan out to Macro's notification pipeline; a no-op implementation is
/// valid where notifications are not wired.
pub trait EscalationNotifier: Send + Sync + 'static {
    /// Notify users that an escalation needs their attention.
    fn notify_assigned(
        &self,
        escalation: &Escalation,
        recipient_user_ids: &[String],
    ) -> impl Future<Output = Result<()>> + Send;
}
