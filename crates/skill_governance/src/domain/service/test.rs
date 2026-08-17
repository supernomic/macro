use std::sync::Mutex;

use super::*;
use crate::domain::model::{NewProposal, ProposalKind, ProposalStatus, SkillScope, content_hash};
use crate::domain::ports::{ProposalFilter, SkillFilter};
use chrono::Utc;
use macro_uuid::Uuid;

#[derive(Default)]
struct FakeSkills {
    skills: Mutex<Vec<SkillRecord>>,
    snapshots: Mutex<Vec<SkillSnapshot>>,
}

impl SkillRepo for FakeSkills {
    async fn insert(&self, skill: &SkillRecord) -> Result<()> {
        self.skills.lock().unwrap().push(skill.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<SkillRecord>> {
        Ok(self
            .skills
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == id)
            .cloned())
    }

    async fn find_by_slug(
        &self,
        org_id: Option<i32>,
        scope: SkillScope,
        slug: &str,
        owner_user_id: Option<&str>,
        owner_team_id: Option<Uuid>,
    ) -> Result<Option<SkillRecord>> {
        Ok(self
            .skills
            .lock()
            .unwrap()
            .iter()
            .find(|s| {
                s.org_id == org_id
                    && s.scope == scope
                    && s.slug == slug
                    && s.owner_user_id.as_deref() == owner_user_id
                    && s.owner_team_id == owner_team_id
                    && s.archived_at.is_none()
            })
            .cloned())
    }

    async fn list(&self, filter: &SkillFilter) -> Result<Vec<SkillRecord>> {
        Ok(self
            .skills
            .lock()
            .unwrap()
            .iter()
            .filter(|s| {
                filter
                    .org_id
                    .is_none_or(|o| s.org_id == Some(o) || s.org_id.is_none())
            })
            .cloned()
            .take(filter.limit.max(1) as usize)
            .collect())
    }

    async fn update(&self, skill: &SkillRecord) -> Result<()> {
        let mut skills = self.skills.lock().unwrap();
        if let Some(existing) = skills.iter_mut().find(|s| s.id == skill.id) {
            *existing = skill.clone();
        }
        Ok(())
    }

    async fn insert_snapshot(&self, snapshot: &SkillSnapshot) -> Result<()> {
        self.snapshots.lock().unwrap().push(snapshot.clone());
        Ok(())
    }

    async fn get_snapshot(&self, id: Uuid) -> Result<Option<SkillSnapshot>> {
        Ok(self
            .snapshots
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == id)
            .cloned())
    }
}

#[derive(Default)]
struct FakeProposals {
    rows: Mutex<Vec<SkillProposal>>,
    evals: Mutex<Vec<SkillEvalRun>>,
    refinements: Mutex<Vec<TraceRefinement>>,
}

impl ProposalRepo for FakeProposals {
    async fn insert(&self, proposal: &SkillProposal) -> Result<()> {
        self.rows.lock().unwrap().push(proposal.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<SkillProposal>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.id == id)
            .cloned())
    }

    async fn list(&self, filter: &ProposalFilter) -> Result<Vec<SkillProposal>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|p| {
                filter.status.is_none_or(|s| p.status == s)
                    && filter
                        .assignee_user_id
                        .as_ref()
                        .is_none_or(|u| p.assignee_user_id.as_ref() == Some(u))
                    && filter
                        .assignee_team_id
                        .is_none_or(|t| p.assignee_team_id == Some(t))
            })
            .cloned()
            .collect())
    }

    async fn update(&self, proposal: &SkillProposal) -> Result<()> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(existing) = rows.iter_mut().find(|p| p.id == proposal.id) {
            *existing = proposal.clone();
        }
        Ok(())
    }

    async fn insert_eval(&self, run: &SkillEvalRun) -> Result<()> {
        self.evals.lock().unwrap().push(run.clone());
        Ok(())
    }

    async fn latest_eval_for_proposal(&self, proposal_id: Uuid) -> Result<Option<SkillEvalRun>> {
        Ok(self
            .evals
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.proposal_id == Some(proposal_id))
            .max_by_key(|e| e.created_at)
            .cloned())
    }

    async fn insert_refinement(&self, job: &TraceRefinement) -> Result<()> {
        self.refinements.lock().unwrap().push(job.clone());
        Ok(())
    }
}

struct FakeTeams;

impl TeamMembershipPort for FakeTeams {
    async fn user_teams(&self, _user_id: &str) -> Result<Vec<Uuid>> {
        Ok(vec![])
    }
}

fn svc() -> SkillGovernanceServiceImpl<FakeSkills, FakeProposals, FakeTeams> {
    SkillGovernanceServiceImpl::new(FakeSkills::default(), FakeProposals::default(), FakeTeams)
}

fn user_create(slug: &str) -> NewProposal {
    NewProposal {
        kind: ProposalKind::Create,
        skill_id: None,
        slug: slug.to_string(),
        target_scope: SkillScope::User,
        owner_user_id: Some("user-1".to_string()),
        owner_team_id: None,
        proposed_name: slug.to_string(),
        proposed_description: "Use when testing.".to_string(),
        proposed_body: "# hello".to_string(),
        diff_summary: "new skill".to_string(),
        evidence: serde_json::json!([]),
        assignee_user_id: None,
        assignee_team_id: None,
    }
}

fn org_create(slug: &str) -> NewProposal {
    let mut p = user_create(slug);
    p.target_scope = SkillScope::Org;
    p.owner_user_id = None;
    p
}

#[tokio::test]
async fn user_scope_create_auto_applies() {
    let svc = svc();
    let proposal = svc
        .propose(Some(1), user_create("refunds"), None, Some("user-1".into()))
        .await
        .unwrap();
    assert_eq!(proposal.status, ProposalStatus::Approved);
    assert!(proposal.skill_id.is_some());
    let catalog = svc.catalog_for(Some(1), Some("user-1"), &[]).await.unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].slug, "refunds");
}

#[tokio::test]
async fn catalog_does_not_leak_other_users_personal_skills() {
    let svc = svc();
    svc.propose(Some(1), user_create("mine"), None, Some("user-1".into()))
        .await
        .unwrap();
    let other = svc.catalog_for(Some(1), Some("user-2"), &[]).await.unwrap();
    assert!(other.is_empty());
    let agent_view = svc.catalog_for(Some(1), None, &[]).await.unwrap();
    assert!(agent_view.is_empty());
}

#[tokio::test]
async fn org_scope_create_stays_pending() {
    let svc = svc();
    let proposal = svc
        .propose(
            Some(1),
            org_create("runbooks"),
            Some("agent-1".into()),
            None,
        )
        .await
        .unwrap();
    assert_eq!(proposal.status, ProposalStatus::Pending);
    let catalog = svc.catalog_for(Some(1), None, &[]).await.unwrap();
    assert!(catalog.is_empty());
}

#[tokio::test]
async fn org_approve_applies_and_rollback_restores_snapshot() {
    let svc = svc();
    let pending = svc
        .propose(
            Some(1),
            org_create("runbooks"),
            Some("agent-1".into()),
            None,
        )
        .await
        .unwrap();
    let approved = svc
        .decide(&Caller::User("reviewer".into()), pending.id, true, None)
        .await
        .unwrap();
    assert_eq!(approved.status, ProposalStatus::Approved);
    let skill_id = approved.skill_id.unwrap();

    // Patch so rollback has a different current body.
    let patch = NewProposal {
        kind: ProposalKind::Patch,
        skill_id: Some(skill_id),
        slug: "runbooks".to_string(),
        target_scope: SkillScope::Org,
        owner_user_id: None,
        owner_team_id: None,
        proposed_name: "runbooks".to_string(),
        proposed_description: "updated".to_string(),
        proposed_body: "# goodbye".to_string(),
        diff_summary: "rewrite".to_string(),
        evidence: serde_json::json!([]),
        assignee_user_id: None,
        assignee_team_id: None,
    };
    let patch_pending = svc
        .propose(Some(1), patch, Some("agent-1".into()), None)
        .await
        .unwrap();
    let patch_approved = svc
        .decide(&Caller::Internal, patch_pending.id, true, None)
        .await
        .unwrap();
    let after_patch = svc.get_skill(skill_id).await.unwrap();
    assert_eq!(after_patch.body, "# goodbye");

    let restored = svc
        .rollback(&Caller::Internal, patch_approved.id, Some("revert".into()))
        .await
        .unwrap();
    assert_eq!(restored.body, "# hello");
    assert_eq!(content_hash("# hello"), restored.content_hash);
}

#[tokio::test]
async fn eval_gate_blocks_org_promotion_when_eval_failed() {
    let svc = svc();
    let pending = svc
        .propose(Some(1), org_create("gated"), Some("agent-1".into()), None)
        .await
        .unwrap();
    svc.record_eval(SkillEvalRun {
        id: macro_uuid::generate_uuid_v7(),
        org_id: Some(1),
        skill_id: None,
        proposal_id: Some(pending.id),
        composition_id: "super-agent/v1".into(),
        dataset: "techops-gold".into(),
        passed: false,
        score: Some(0.1),
        report: serde_json::json!({}),
        created_at: Utc::now(),
    })
    .await
    .unwrap();

    let err = svc
        .decide(&Caller::Internal, pending.id, true, None)
        .await
        .unwrap_err();
    assert!(matches!(err, GovernanceError::EvalGate(_)));
}

#[tokio::test]
async fn refine_opens_a_proposal() {
    let svc = svc();
    let (job, proposal) = svc
        .refine(
            Some(1),
            org_create("from-trace"),
            Utc::now() - chrono::Duration::days(7),
            Utc::now(),
            None,
            serde_json::json!([{"seq": 12}]),
        )
        .await
        .unwrap();
    assert_eq!(job.proposal_id, Some(proposal.id));
    assert_eq!(proposal.status, ProposalStatus::Pending);
    assert_eq!(proposal.proposer_agent_id.as_deref(), Some("trace-refine"));
}

#[tokio::test]
async fn empty_slug_is_rejected() {
    let svc = svc();
    let mut p = user_create("x");
    p.slug = "  ".into();
    let err = svc
        .propose(Some(1), p, None, Some("user-1".into()))
        .await
        .unwrap_err();
    assert!(matches!(err, GovernanceError::InvalidRequest(_)));
}
