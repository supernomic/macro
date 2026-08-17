//! Domain service: catalog serving, staged proposals, eval-gated promotion,
//! snapshots, and one-step rollback.

#[cfg(test)]
mod test;

use chrono::Utc;
use macro_uuid::Uuid;

use super::model::{
    GovernanceError, NewProposal, OkfStatus, ProposalKind, ProposalStatus, Result,
    SkillCatalogEntry, SkillEvalRun, SkillProposal, SkillRecord, SkillScope, SkillSnapshot,
    TraceRefinement, TrustTier, content_hash,
};
use super::ports::{ProposalFilter, ProposalRepo, SkillFilter, SkillRepo, TeamMembershipPort};

const DEFAULT_LIST_LIMIT: i64 = 100;

/// A caller identity for user-facing operations.
#[derive(Debug, Clone)]
pub enum Caller {
    /// A Macro user.
    User(String),
    /// A trusted internal caller.
    Internal,
}

impl Caller {
    fn actor_id(&self) -> String {
        match self {
            Caller::User(id) => id.clone(),
            Caller::Internal => "internal".to_string(),
        }
    }
}

/// Personal inbox view for skill proposals.
#[derive(Debug, Clone)]
pub struct UserProposals {
    /// Pending proposals assigned directly to the user.
    pub assigned: Vec<SkillProposal>,
    /// Pending proposals on the user's teams' queues.
    pub team_queue: Vec<SkillProposal>,
}

/// Domain service exposed to inbound adapters.
pub trait SkillGovernanceService: Send + Sync + 'static {
    /// Catalog of skills the caller may inject (user + team + org + platform).
    fn catalog_for(
        &self,
        org_id: Option<i32>,
        user_id: Option<&str>,
        team_ids: &[Uuid],
    ) -> impl Future<Output = Result<Vec<SkillCatalogEntry>>> + Send;

    /// Fetch one skill by id.
    fn get_skill(&self, id: Uuid) -> impl Future<Output = Result<SkillRecord>> + Send;

    /// Open a staged proposal. User-scope creates may auto-apply.
    fn propose(
        &self,
        org_id: Option<i32>,
        proposal: NewProposal,
        proposer_agent_id: Option<String>,
        proposer_user_id: Option<String>,
    ) -> impl Future<Output = Result<SkillProposal>> + Send;

    /// Fetch one proposal.
    fn get_proposal(
        &self,
        caller: &Caller,
        id: Uuid,
    ) -> impl Future<Output = Result<SkillProposal>> + Send;

    /// The caller's personal proposal inbox.
    fn list_for_user(&self, user_id: &str) -> impl Future<Output = Result<UserProposals>> + Send;

    /// Approve or reject a pending proposal. Org/team promotion is gated on
    /// evals when an eval run exists for the proposal.
    fn decide(
        &self,
        caller: &Caller,
        id: Uuid,
        approved: bool,
        note: Option<String>,
    ) -> impl Future<Output = Result<SkillProposal>> + Send;

    /// Roll a previously approved proposal back to its snapshot.
    fn rollback(
        &self,
        caller: &Caller,
        id: Uuid,
        note: Option<String>,
    ) -> impl Future<Output = Result<SkillRecord>> + Send;

    /// Record an eval run that may later gate promotion.
    fn record_eval(&self, run: SkillEvalRun) -> impl Future<Output = Result<SkillEvalRun>> + Send;

    /// Ingest a trace-refinement job: persist the job and open a proposal.
    fn refine(
        &self,
        org_id: Option<i32>,
        proposal: NewProposal,
        window_start: chrono::DateTime<Utc>,
        window_end: chrono::DateTime<Utc>,
        session_id: Option<Uuid>,
        evidence_excerpt: serde_json::Value,
    ) -> impl Future<Output = Result<(TraceRefinement, SkillProposal)>> + Send;
}

/// Concrete governance service over the storage ports.
#[derive(Debug, Clone)]
pub struct SkillGovernanceServiceImpl<S, P, T> {
    skills: S,
    proposals: P,
    teams: T,
}

impl<S, P, T> SkillGovernanceServiceImpl<S, P, T>
where
    S: SkillRepo,
    P: ProposalRepo,
    T: TeamMembershipPort,
{
    /// Build the service over its ports.
    pub fn new(skills: S, proposals: P, teams: T) -> Self {
        Self {
            skills,
            proposals,
            teams,
        }
    }

    fn new_id() -> Uuid {
        macro_uuid::generate_uuid_v7()
    }

    async fn apply_proposal(
        &self,
        proposal: &SkillProposal,
        actor_id: &str,
    ) -> Result<(SkillRecord, Option<Uuid>)> {
        let now = Utc::now();
        match proposal.kind {
            ProposalKind::Create => {
                let skill = SkillRecord {
                    id: Self::new_id(),
                    org_id: proposal.org_id,
                    scope: proposal.target_scope,
                    owner_user_id: proposal.owner_user_id.clone(),
                    owner_team_id: proposal.owner_team_id,
                    slug: proposal.slug.clone(),
                    name: proposal.proposed_name.clone(),
                    description: proposal.proposed_description.clone(),
                    body: proposal.proposed_body.clone(),
                    trust_tier: if proposal.target_scope == SkillScope::User {
                        TrustTier::Untrusted
                    } else {
                        TrustTier::Verified
                    },
                    okf_type: "skill".to_string(),
                    okf_sources: Vec::new(),
                    okf_generated: proposal.proposer_agent_id.is_some(),
                    okf_verified: proposal.target_scope.requires_review(),
                    okf_status: OkfStatus::Active,
                    stale_after: None,
                    content_hash: content_hash(&proposal.proposed_body),
                    version: 1,
                    created_at: now,
                    updated_at: now,
                    archived_at: None,
                };
                self.skills.insert(&skill).await?;
                let snapshot = SkillSnapshot {
                    id: Self::new_id(),
                    skill_id: skill.id,
                    version: skill.version,
                    body: skill.body.clone(),
                    description: skill.description.clone(),
                    content_hash: skill.content_hash.clone(),
                    created_at: now,
                    created_by: actor_id.to_string(),
                };
                self.skills.insert_snapshot(&snapshot).await?;
                Ok((skill, Some(snapshot.id)))
            }
            ProposalKind::Patch => {
                let skill_id = proposal.skill_id.ok_or_else(|| {
                    GovernanceError::InvalidRequest("patch requires skill_id".to_string())
                })?;
                let mut skill = self
                    .skills
                    .get(skill_id)
                    .await?
                    .ok_or(GovernanceError::NotFound)?;
                let snapshot = SkillSnapshot {
                    id: Self::new_id(),
                    skill_id: skill.id,
                    version: skill.version,
                    body: skill.body.clone(),
                    description: skill.description.clone(),
                    content_hash: skill.content_hash.clone(),
                    created_at: now,
                    created_by: actor_id.to_string(),
                };
                self.skills.insert_snapshot(&snapshot).await?;
                skill.description = proposal.proposed_description.clone();
                skill.body = proposal.proposed_body.clone();
                skill.name = proposal.proposed_name.clone();
                skill.content_hash = content_hash(&skill.body);
                skill.version += 1;
                skill.updated_at = now;
                skill.okf_generated = proposal.proposer_agent_id.is_some();
                self.skills.update(&skill).await?;
                Ok((skill, Some(snapshot.id)))
            }
            ProposalKind::Archive => {
                let skill_id = proposal.skill_id.ok_or_else(|| {
                    GovernanceError::InvalidRequest("archive requires skill_id".to_string())
                })?;
                let mut skill = self
                    .skills
                    .get(skill_id)
                    .await?
                    .ok_or(GovernanceError::NotFound)?;
                let snapshot = SkillSnapshot {
                    id: Self::new_id(),
                    skill_id: skill.id,
                    version: skill.version,
                    body: skill.body.clone(),
                    description: skill.description.clone(),
                    content_hash: skill.content_hash.clone(),
                    created_at: now,
                    created_by: actor_id.to_string(),
                };
                self.skills.insert_snapshot(&snapshot).await?;
                skill.okf_status = OkfStatus::Archived;
                skill.archived_at = Some(now);
                skill.updated_at = now;
                self.skills.update(&skill).await?;
                Ok((skill, Some(snapshot.id)))
            }
        }
    }
}

/// Whether a skill belongs in this caller's catalog. Platform skills are
/// global; org skills are tenant-wide; user/team skills are owner-scoped
/// and never leak to other principals.
fn visible_in_catalog(
    skill: &SkillRecord,
    org_id: Option<i32>,
    user_id: Option<&str>,
    team_ids: &[Uuid],
) -> bool {
    if skill.okf_status != OkfStatus::Active || skill.archived_at.is_some() {
        return false;
    }
    match skill.scope {
        SkillScope::Platform => true,
        SkillScope::Org => skill.org_id == org_id,
        SkillScope::User => {
            user_id.is_some() && skill.org_id == org_id && skill.owner_user_id.as_deref() == user_id
        }
        SkillScope::Team => {
            skill.org_id == org_id
                && skill
                    .owner_team_id
                    .is_some_and(|tid| team_ids.contains(&tid))
        }
    }
}

impl<S, P, T> SkillGovernanceService for SkillGovernanceServiceImpl<S, P, T>
where
    S: SkillRepo,
    P: ProposalRepo,
    T: TeamMembershipPort,
{
    #[tracing::instrument(skip(self), err)]
    async fn catalog_for(
        &self,
        org_id: Option<i32>,
        user_id: Option<&str>,
        team_ids: &[Uuid],
    ) -> Result<Vec<SkillCatalogEntry>> {
        let skills = self
            .skills
            .list(&SkillFilter {
                org_id,
                owner_user_id: user_id.map(str::to_string),
                owner_team_ids: team_ids.to_vec(),
                limit: DEFAULT_LIST_LIMIT,
                ..Default::default()
            })
            .await?;
        Ok(skills
            .iter()
            .filter(|s| visible_in_catalog(s, org_id, user_id, team_ids))
            .map(SkillCatalogEntry::from)
            .collect())
    }

    #[tracing::instrument(skip(self), err)]
    async fn get_skill(&self, id: Uuid) -> Result<SkillRecord> {
        self.skills.get(id).await?.ok_or(GovernanceError::NotFound)
    }

    #[tracing::instrument(skip(self, proposal), err)]
    async fn propose(
        &self,
        org_id: Option<i32>,
        proposal: NewProposal,
        proposer_agent_id: Option<String>,
        proposer_user_id: Option<String>,
    ) -> Result<SkillProposal> {
        if proposal.slug.trim().is_empty() {
            return Err(GovernanceError::InvalidRequest(
                "slug must not be empty".to_string(),
            ));
        }
        if matches!(proposal.kind, ProposalKind::Patch | ProposalKind::Archive)
            && proposal.skill_id.is_none()
        {
            return Err(GovernanceError::InvalidRequest(
                "patch/archive requires skill_id".to_string(),
            ));
        }
        let now = Utc::now();
        let mut row = SkillProposal {
            id: Self::new_id(),
            org_id,
            skill_id: proposal.skill_id,
            kind: proposal.kind,
            slug: proposal.slug,
            target_scope: proposal.target_scope,
            owner_user_id: proposal.owner_user_id,
            owner_team_id: proposal.owner_team_id,
            proposed_name: proposal.proposed_name,
            proposed_description: proposal.proposed_description,
            proposed_body: proposal.proposed_body,
            diff_summary: proposal.diff_summary,
            evidence: proposal.evidence,
            proposer_agent_id,
            proposer_user_id: proposer_user_id.clone(),
            status: ProposalStatus::Pending,
            assignee_user_id: proposal.assignee_user_id,
            assignee_team_id: proposal.assignee_team_id,
            snapshot_id: None,
            eval_run_id: None,
            eval_passed: None,
            decided_by: None,
            decision_note: None,
            decided_at: None,
            created_at: now,
            updated_at: now,
        };

        // Personal-scope creates auto-apply; team/org always wait for review.
        if !row.target_scope.requires_review() && row.kind == ProposalKind::Create {
            let actor = proposer_user_id
                .clone()
                .or_else(|| row.proposer_agent_id.clone())
                .unwrap_or_else(|| "system".to_string());
            let (skill, snapshot_id) = self.apply_proposal(&row, &actor).await?;
            row.skill_id = Some(skill.id);
            row.snapshot_id = snapshot_id;
            row.status = ProposalStatus::Approved;
            row.decided_by = Some(actor);
            row.decided_at = Some(now);
            row.updated_at = now;
        }

        self.proposals.insert(&row).await?;
        Ok(row)
    }

    #[tracing::instrument(skip(self), err)]
    async fn get_proposal(&self, _caller: &Caller, id: Uuid) -> Result<SkillProposal> {
        self.proposals
            .get(id)
            .await?
            .ok_or(GovernanceError::NotFound)
    }

    #[tracing::instrument(skip(self), err)]
    async fn list_for_user(&self, user_id: &str) -> Result<UserProposals> {
        let assigned = self
            .proposals
            .list(&ProposalFilter {
                assignee_user_id: Some(user_id.to_string()),
                status: Some(ProposalStatus::Pending),
                limit: DEFAULT_LIST_LIMIT,
                ..Default::default()
            })
            .await?;
        let mut team_queue = Vec::new();
        for team_id in self.teams.user_teams(user_id).await? {
            let mut items = self
                .proposals
                .list(&ProposalFilter {
                    assignee_team_id: Some(team_id),
                    status: Some(ProposalStatus::Pending),
                    limit: DEFAULT_LIST_LIMIT,
                    ..Default::default()
                })
                .await?;
            team_queue.append(&mut items);
        }
        Ok(UserProposals {
            assigned,
            team_queue,
        })
    }

    #[tracing::instrument(skip(self, caller), err)]
    async fn decide(
        &self,
        caller: &Caller,
        id: Uuid,
        approved: bool,
        note: Option<String>,
    ) -> Result<SkillProposal> {
        let mut proposal = self
            .proposals
            .get(id)
            .await?
            .ok_or(GovernanceError::NotFound)?;
        if proposal.status != ProposalStatus::Pending {
            return Err(GovernanceError::InvalidStatus(
                "proposal is not pending".to_string(),
            ));
        }

        if approved
            && proposal.target_scope.requires_review()
            && let Some(eval) = self.proposals.latest_eval_for_proposal(proposal.id).await?
        {
            if !eval.passed {
                return Err(GovernanceError::EvalGate(
                    "latest eval run did not pass".to_string(),
                ));
            }
            proposal.eval_run_id = Some(eval.id.to_string());
            proposal.eval_passed = Some(true);
        }

        let actor = caller.actor_id();
        let now = Utc::now();
        if approved {
            let (skill, snapshot_id) = self.apply_proposal(&proposal, &actor).await?;
            proposal.skill_id = Some(skill.id);
            proposal.snapshot_id = snapshot_id;
            proposal.status = ProposalStatus::Approved;
        } else {
            proposal.status = ProposalStatus::Rejected;
        }
        proposal.decided_by = Some(actor);
        proposal.decision_note = note;
        proposal.decided_at = Some(now);
        proposal.updated_at = now;
        self.proposals.update(&proposal).await?;
        Ok(proposal)
    }

    #[tracing::instrument(skip(self, caller), err)]
    async fn rollback(
        &self,
        caller: &Caller,
        id: Uuid,
        note: Option<String>,
    ) -> Result<SkillRecord> {
        let mut proposal = self
            .proposals
            .get(id)
            .await?
            .ok_or(GovernanceError::NotFound)?;
        if proposal.status != ProposalStatus::Approved {
            return Err(GovernanceError::InvalidStatus(
                "only approved proposals can be rolled back".to_string(),
            ));
        }
        let snapshot_id = proposal.snapshot_id.ok_or_else(|| {
            GovernanceError::InvalidRequest("proposal has no snapshot to restore".to_string())
        })?;
        let snapshot = self
            .skills
            .get_snapshot(snapshot_id)
            .await?
            .ok_or(GovernanceError::NotFound)?;
        let skill_id = proposal.skill_id.ok_or(GovernanceError::NotFound)?;
        let mut skill = self
            .skills
            .get(skill_id)
            .await?
            .ok_or(GovernanceError::NotFound)?;

        skill.body = snapshot.body;
        skill.description = snapshot.description;
        skill.content_hash = snapshot.content_hash;
        skill.version = snapshot.version;
        skill.okf_status = OkfStatus::Active;
        skill.archived_at = None;
        skill.updated_at = Utc::now();
        self.skills.update(&skill).await?;

        proposal.status = ProposalStatus::RolledBack;
        proposal.decision_note = note;
        proposal.decided_by = Some(caller.actor_id());
        proposal.decided_at = Some(Utc::now());
        proposal.updated_at = Utc::now();
        self.proposals.update(&proposal).await?;
        Ok(skill)
    }

    #[tracing::instrument(skip(self, run), err)]
    async fn record_eval(&self, run: SkillEvalRun) -> Result<SkillEvalRun> {
        self.proposals.insert_eval(&run).await?;
        Ok(run)
    }

    #[tracing::instrument(skip(self, proposal, evidence_excerpt), err)]
    async fn refine(
        &self,
        org_id: Option<i32>,
        proposal: NewProposal,
        window_start: chrono::DateTime<Utc>,
        window_end: chrono::DateTime<Utc>,
        session_id: Option<Uuid>,
        evidence_excerpt: serde_json::Value,
    ) -> Result<(TraceRefinement, SkillProposal)> {
        let opened = self
            .propose(org_id, proposal, Some("trace-refine".to_string()), None)
            .await?;
        let job = TraceRefinement {
            id: Self::new_id(),
            org_id,
            proposal_id: Some(opened.id),
            session_id,
            window_start,
            window_end,
            evidence_excerpt,
            created_at: Utc::now(),
        };
        self.proposals.insert_refinement(&job).await?;
        Ok((job, opened))
    }
}
