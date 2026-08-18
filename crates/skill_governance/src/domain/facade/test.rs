use std::sync::Mutex;

use agent_identity::domain::model::{AgentKind, AgentPrincipal, VerifiedAgent};
use chrono::Utc;
use macro_uuid::Uuid;

use super::*;
use crate::domain::model::{
    NewProposal, OkfStatus, ProposalKind, ProposalStatus, SkillEvalRun, SkillRecord, SkillScope,
    TraceRefinement, TrustTier, content_hash,
};
use crate::domain::service::{Caller, SkillGovernanceService};

struct RejectAll;

impl SkillGovernanceService for RejectAll {
    async fn catalog_for(
        &self,
        _org_id: Option<i32>,
        _user_id: Option<&str>,
        _team_ids: &[Uuid],
    ) -> Result<Vec<SkillCatalogEntry>> {
        Ok(vec![])
    }

    async fn get_skill(
        &self,
        _org_id: Option<i32>,
        _user_id: Option<&str>,
        _team_ids: &[Uuid],
        _id: Uuid,
    ) -> Result<SkillRecord> {
        Err(GovernanceError::NotFound)
    }

    async fn propose(
        &self,
        _org_id: Option<i32>,
        _proposal: NewProposal,
        _proposer_agent_id: Option<String>,
        _proposer_user_id: Option<String>,
    ) -> Result<SkillProposal> {
        Err(GovernanceError::NotFound)
    }

    async fn get_proposal(&self, _caller: &Caller, _id: Uuid) -> Result<SkillProposal> {
        Err(GovernanceError::NotFound)
    }

    async fn list_for_user(&self, _user_id: &str) -> Result<crate::domain::service::UserProposals> {
        Ok(crate::domain::service::UserProposals {
            assigned: vec![],
            team_queue: vec![],
        })
    }

    async fn decide(
        &self,
        _caller: &Caller,
        _id: Uuid,
        _approved: bool,
        _note: Option<String>,
    ) -> Result<SkillProposal> {
        Err(GovernanceError::NotFound)
    }

    async fn rollback(
        &self,
        _caller: &Caller,
        _id: Uuid,
        _note: Option<String>,
    ) -> Result<SkillRecord> {
        Err(GovernanceError::NotFound)
    }

    async fn record_eval(&self, run: SkillEvalRun) -> Result<SkillEvalRun> {
        Ok(run)
    }

    async fn refine(
        &self,
        _org_id: Option<i32>,
        _proposal: NewProposal,
        _window_start: chrono::DateTime<Utc>,
        _window_end: chrono::DateTime<Utc>,
        _session_id: Option<Uuid>,
        _evidence_excerpt: serde_json::Value,
    ) -> Result<(TraceRefinement, SkillProposal)> {
        Err(GovernanceError::NotFound)
    }
}

#[derive(Default)]
struct FakeService {
    skills: Mutex<Vec<SkillRecord>>,
    proposals: Mutex<Vec<SkillProposal>>,
}

impl FakeService {
    fn push_skill(&self, skill: SkillRecord) {
        self.skills.lock().unwrap().push(skill);
    }

    fn push_proposal(&self, proposal: SkillProposal) {
        self.proposals.lock().unwrap().push(proposal);
    }
}

fn sample_skill(
    org_id: Option<i32>,
    scope: SkillScope,
    owner_user_id: Option<&str>,
) -> SkillRecord {
    let now = Utc::now();
    let body = "# body".to_string();
    SkillRecord {
        id: macro_uuid::generate_uuid_v7(),
        org_id,
        scope,
        owner_user_id: owner_user_id.map(str::to_string),
        owner_team_id: None,
        slug: "skill".into(),
        name: "skill".into(),
        description: "d".into(),
        body: body.clone(),
        trust_tier: TrustTier::Verified,
        okf_type: "skill".into(),
        okf_sources: vec![],
        okf_generated: false,
        okf_verified: true,
        okf_status: OkfStatus::Active,
        stale_after: None,
        content_hash: content_hash(&body),
        version: 1,
        created_at: now,
        updated_at: now,
        archived_at: None,
    }
}

fn sample_proposal(org_id: Option<i32>) -> SkillProposal {
    let now = Utc::now();
    SkillProposal {
        id: macro_uuid::generate_uuid_v7(),
        org_id,
        skill_id: None,
        kind: ProposalKind::Create,
        slug: "p".into(),
        target_scope: SkillScope::Org,
        owner_user_id: None,
        owner_team_id: None,
        proposed_name: "p".into(),
        proposed_description: "d".into(),
        proposed_body: "# b".into(),
        diff_summary: "s".into(),
        evidence: serde_json::json!([]),
        proposer_agent_id: Some("agent".into()),
        proposer_user_id: None,
        status: ProposalStatus::Pending,
        assignee_user_id: None,
        assignee_team_id: None,
        snapshot_id: None,
        eval_run_id: None,
        eval_passed: None,
        decided_by: None,
        decision_note: None,
        decided_at: None,
        created_at: now,
        updated_at: now,
    }
}

impl SkillGovernanceService for FakeService {
    async fn catalog_for(
        &self,
        org_id: Option<i32>,
        _user_id: Option<&str>,
        _team_ids: &[Uuid],
    ) -> Result<Vec<SkillCatalogEntry>> {
        Ok(self
            .skills
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.org_id == org_id || s.scope == SkillScope::Platform)
            .map(SkillCatalogEntry::from)
            .collect())
    }

    async fn get_skill(
        &self,
        org_id: Option<i32>,
        user_id: Option<&str>,
        team_ids: &[Uuid],
        id: Uuid,
    ) -> Result<SkillRecord> {
        let skill = self
            .skills
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or(GovernanceError::NotFound)?;
        let visible = match skill.scope {
            SkillScope::Platform => true,
            SkillScope::Org => skill.org_id == org_id,
            SkillScope::User => {
                user_id.is_some()
                    && skill.org_id == org_id
                    && skill.owner_user_id.as_deref() == user_id
            }
            SkillScope::Team => {
                skill.org_id == org_id
                    && skill
                        .owner_team_id
                        .is_some_and(|tid| team_ids.contains(&tid))
            }
        };
        if visible {
            Ok(skill)
        } else {
            Err(GovernanceError::NotFound)
        }
    }

    async fn propose(
        &self,
        org_id: Option<i32>,
        proposal: NewProposal,
        proposer_agent_id: Option<String>,
        _proposer_user_id: Option<String>,
    ) -> Result<SkillProposal> {
        let mut row = sample_proposal(org_id);
        row.slug = proposal.slug;
        row.target_scope = proposal.target_scope;
        row.proposer_agent_id = proposer_agent_id;
        self.proposals.lock().unwrap().push(row.clone());
        Ok(row)
    }

    async fn get_proposal(&self, _caller: &Caller, id: Uuid) -> Result<SkillProposal> {
        self.proposals
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or(GovernanceError::NotFound)
    }

    async fn list_for_user(&self, _user_id: &str) -> Result<crate::domain::service::UserProposals> {
        unimplemented!()
    }

    async fn decide(
        &self,
        _caller: &Caller,
        _id: Uuid,
        _approved: bool,
        _note: Option<String>,
    ) -> Result<SkillProposal> {
        unimplemented!()
    }

    async fn rollback(
        &self,
        _caller: &Caller,
        _id: Uuid,
        _note: Option<String>,
    ) -> Result<SkillRecord> {
        unimplemented!()
    }

    async fn record_eval(&self, run: SkillEvalRun) -> Result<SkillEvalRun> {
        Ok(run)
    }

    async fn refine(
        &self,
        _org_id: Option<i32>,
        _proposal: NewProposal,
        _window_start: chrono::DateTime<Utc>,
        _window_end: chrono::DateTime<Utc>,
        _session_id: Option<Uuid>,
        _evidence_excerpt: serde_json::Value,
    ) -> Result<(TraceRefinement, SkillProposal)> {
        unimplemented!()
    }
}

fn agent(org_id: Option<i32>, scopes: &[&str]) -> VerifiedAgent {
    VerifiedAgent {
        principal: AgentPrincipal {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            slug: "super-agent".into(),
            display_name: "Super".into(),
            kind: AgentKind::SuperAgent,
            created_at: Utc::now(),
            disabled_at: None,
        },
        token_id: macro_uuid::generate_uuid_v7(),
        scopes: scopes.iter().map(|s| (*s).to_string()).collect(),
    }
}

fn agent_without_skill_scopes() -> VerifiedAgent {
    agent(Some(1), &["ledger:append"])
}

fn sample_new_proposal() -> NewProposal {
    NewProposal {
        kind: ProposalKind::Create,
        skill_id: None,
        slug: "x".into(),
        target_scope: SkillScope::Org,
        owner_user_id: None,
        owner_team_id: None,
        proposed_name: "x".into(),
        proposed_description: "x".into(),
        proposed_body: "x".into(),
        diff_summary: "x".into(),
        evidence: serde_json::json!([]),
        assignee_user_id: None,
        assignee_team_id: None,
    }
}

#[tokio::test]
async fn catalog_requires_skill_read() {
    let facade = AgentSkillFacade::new(RejectAll);
    let err = facade
        .catalog(&agent_without_skill_scopes())
        .await
        .unwrap_err();
    assert!(matches!(err, GovernanceError::MissingScope { .. }));
}

#[tokio::test]
async fn get_skill_requires_skill_read() {
    let facade = AgentSkillFacade::new(RejectAll);
    let err = facade
        .get_skill(
            &agent_without_skill_scopes(),
            macro_uuid::generate_uuid_v7(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, GovernanceError::MissingScope { .. }));
}

#[tokio::test]
async fn get_proposal_requires_skill_read() {
    let facade = AgentSkillFacade::new(RejectAll);
    let err = facade
        .get_proposal(
            &agent_without_skill_scopes(),
            macro_uuid::generate_uuid_v7(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, GovernanceError::MissingScope { .. }));
}

#[tokio::test]
async fn propose_requires_skill_propose() {
    let facade = AgentSkillFacade::new(RejectAll);
    let err = facade
        .propose(&agent_without_skill_scopes(), sample_new_proposal())
        .await
        .unwrap_err();
    assert!(matches!(err, GovernanceError::MissingScope { .. }));
}

#[tokio::test]
async fn get_skill_hides_other_org_and_personal_skills_but_serves_platform() {
    let svc = FakeService::default();
    let org_skill = sample_skill(Some(1), SkillScope::Org, None);
    let other_org = sample_skill(Some(2), SkillScope::Org, None);
    let personal = sample_skill(Some(1), SkillScope::User, Some("user-1"));
    let mut platform = sample_skill(None, SkillScope::Platform, None);
    platform.slug = "platform".into();
    let org_id = org_skill.id;
    let other_id = other_org.id;
    let personal_id = personal.id;
    let platform_id = platform.id;
    svc.push_skill(org_skill);
    svc.push_skill(other_org);
    svc.push_skill(personal);
    svc.push_skill(platform);

    let facade = AgentSkillFacade::new(svc);
    let caller = agent(Some(1), &[SCOPE_SKILL_READ]);

    assert!(facade.get_skill(&caller, org_id).await.is_ok());
    assert!(facade.get_skill(&caller, platform_id).await.is_ok());
    assert!(matches!(
        facade.get_skill(&caller, other_id).await.unwrap_err(),
        GovernanceError::NotFound
    ));
    assert!(matches!(
        facade.get_skill(&caller, personal_id).await.unwrap_err(),
        GovernanceError::NotFound
    ));
}

#[tokio::test]
async fn get_proposal_enforces_org_tenancy() {
    let svc = FakeService::default();
    let mine = sample_proposal(Some(1));
    let theirs = sample_proposal(Some(2));
    let mine_id = mine.id;
    let theirs_id = theirs.id;
    svc.push_proposal(mine);
    svc.push_proposal(theirs);

    let facade = AgentSkillFacade::new(svc);
    let caller = agent(Some(1), &[SCOPE_SKILL_READ]);

    assert!(facade.get_proposal(&caller, mine_id).await.is_ok());
    assert!(matches!(
        facade.get_proposal(&caller, theirs_id).await.unwrap_err(),
        GovernanceError::NotFound
    ));
}

#[tokio::test]
async fn propose_forces_principal_org() {
    let facade = AgentSkillFacade::new(FakeService::default());
    let caller = agent(Some(7), &[SCOPE_SKILL_PROPOSE]);
    let opened = facade
        .propose(&caller, sample_new_proposal())
        .await
        .unwrap();
    assert_eq!(opened.org_id, Some(7));
    assert_eq!(
        opened.proposer_agent_id,
        Some(caller.principal.id.to_string())
    );
}
