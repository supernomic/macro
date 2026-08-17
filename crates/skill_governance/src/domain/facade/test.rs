use agent_identity::domain::model::{AgentKind, AgentPrincipal, VerifiedAgent};
use chrono::Utc;

use super::*;
use crate::domain::model::{NewProposal, ProposalKind, SkillEvalRun, SkillScope, TraceRefinement};
use crate::domain::service::{Caller, SkillGovernanceService};

struct RejectAll;

impl SkillGovernanceService for RejectAll {
    async fn catalog_for(
        &self,
        _org_id: Option<i32>,
        _user_id: Option<&str>,
        _team_ids: &[macro_uuid::Uuid],
    ) -> Result<Vec<SkillCatalogEntry>> {
        Ok(vec![])
    }

    async fn get_skill(&self, _id: macro_uuid::Uuid) -> Result<SkillRecord> {
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

    async fn get_proposal(&self, _caller: &Caller, _id: macro_uuid::Uuid) -> Result<SkillProposal> {
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
        _id: macro_uuid::Uuid,
        _approved: bool,
        _note: Option<String>,
    ) -> Result<SkillProposal> {
        Err(GovernanceError::NotFound)
    }

    async fn rollback(
        &self,
        _caller: &Caller,
        _id: macro_uuid::Uuid,
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
        _session_id: Option<macro_uuid::Uuid>,
        _evidence_excerpt: serde_json::Value,
    ) -> Result<(TraceRefinement, SkillProposal)> {
        Err(GovernanceError::NotFound)
    }
}

fn agent_without(scope: &str) -> VerifiedAgent {
    VerifiedAgent {
        principal: AgentPrincipal {
            id: macro_uuid::generate_uuid_v7(),
            org_id: Some(1),
            slug: "super-agent".into(),
            display_name: "Super".into(),
            kind: AgentKind::SuperAgent,
            created_at: Utc::now(),
            disabled_at: None,
        },
        token_id: macro_uuid::generate_uuid_v7(),
        scopes: vec!["ledger:append".into()]
            .into_iter()
            .filter(|s| s != scope)
            .collect(),
    }
}

#[tokio::test]
async fn catalog_requires_skill_read() {
    let facade = AgentSkillFacade::new(RejectAll);
    let err = facade
        .catalog(&agent_without("skill:read"))
        .await
        .unwrap_err();
    assert!(matches!(err, GovernanceError::MissingScope { .. }));
}

#[tokio::test]
async fn propose_requires_skill_propose() {
    let facade = AgentSkillFacade::new(RejectAll);
    let err = facade
        .propose(
            &agent_without("skill:propose"),
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
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, GovernanceError::MissingScope { .. }));
}
