//! Ports for skills governance.

use macro_uuid::Uuid;

use super::model::{
    Result, SkillEvalRun, SkillProposal, SkillRecord, SkillScope, SkillSnapshot, TraceRefinement,
};

/// Filter for listing skills an agent or user may inject.
#[derive(Debug, Clone, Default)]
pub struct SkillFilter {
    /// Restrict to an organization.
    pub org_id: Option<i32>,
    /// Include user-scoped skills owned by this user.
    pub owner_user_id: Option<String>,
    /// Include team-scoped skills owned by these teams.
    pub owner_team_ids: Vec<Uuid>,
    /// Restrict to a scope.
    pub scope: Option<SkillScope>,
    /// Maximum rows.
    pub limit: i64,
}

/// Filter for listing proposals.
#[derive(Debug, Clone, Default)]
pub struct ProposalFilter {
    /// Restrict to an organization.
    pub org_id: Option<i32>,
    /// Restrict to a status.
    pub status: Option<super::model::ProposalStatus>,
    /// Direct assignee.
    pub assignee_user_id: Option<String>,
    /// Team queue.
    pub assignee_team_id: Option<Uuid>,
    /// Restrict to items with no direct assignee (team-queue HITL).
    pub unassigned_only: bool,
    /// Restrict to this slug (pending-proposal idempotency).
    pub slug: Option<String>,
    /// Maximum rows.
    pub limit: i64,
}

/// Storage port for governed skills and snapshots.
pub trait SkillRepo: Send + Sync + 'static {
    /// Insert a new skill.
    fn insert(&self, skill: &SkillRecord) -> impl Future<Output = Result<()>> + Send;

    /// Fetch one skill.
    fn get(&self, id: Uuid) -> impl Future<Output = Result<Option<SkillRecord>>> + Send;

    /// Look up by org + scope + slug (active only).
    fn find_by_slug(
        &self,
        org_id: Option<i32>,
        scope: SkillScope,
        slug: &str,
        owner_user_id: Option<&str>,
        owner_team_id: Option<Uuid>,
    ) -> impl Future<Output = Result<Option<SkillRecord>>> + Send;

    /// List skills matching a filter (active first).
    fn list(&self, filter: &SkillFilter) -> impl Future<Output = Result<Vec<SkillRecord>>> + Send;

    /// Replace body/description/provenance of an existing skill.
    fn update(&self, skill: &SkillRecord) -> impl Future<Output = Result<()>> + Send;

    /// Insert a snapshot.
    fn insert_snapshot(&self, snapshot: &SkillSnapshot) -> impl Future<Output = Result<()>> + Send;

    /// Fetch one snapshot.
    fn get_snapshot(&self, id: Uuid) -> impl Future<Output = Result<Option<SkillSnapshot>>> + Send;
}

/// Storage port for staged proposals, evals, and refinement jobs.
pub trait ProposalRepo: Send + Sync + 'static {
    /// Insert a proposal.
    fn insert(&self, proposal: &SkillProposal) -> impl Future<Output = Result<()>> + Send;

    /// Fetch one proposal.
    fn get(&self, id: Uuid) -> impl Future<Output = Result<Option<SkillProposal>>> + Send;

    /// List proposals matching a filter, newest first.
    fn list(
        &self,
        filter: &ProposalFilter,
    ) -> impl Future<Output = Result<Vec<SkillProposal>>> + Send;

    /// Persist an updated proposal row.
    fn update(&self, proposal: &SkillProposal) -> impl Future<Output = Result<()>> + Send;

    /// Record an eval run.
    fn insert_eval(&self, run: &SkillEvalRun) -> impl Future<Output = Result<()>> + Send;

    /// Latest eval for a proposal, if any.
    fn latest_eval_for_proposal(
        &self,
        proposal_id: Uuid,
    ) -> impl Future<Output = Result<Option<SkillEvalRun>>> + Send;

    /// Record a trace-refinement job.
    fn insert_refinement(&self, job: &TraceRefinement) -> impl Future<Output = Result<()>> + Send;
}

/// Port over team membership (backed by MacroDB `team_user`).
pub trait TeamMembershipPort: Send + Sync + 'static {
    /// Team ids the user belongs to.
    fn user_teams(&self, user_id: &str) -> impl Future<Output = Result<Vec<Uuid>>> + Send;
}
