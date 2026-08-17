use std::sync::Mutex;

use agent_identity::domain::model::{AgentKind, AgentPrincipal, VerifiedAgent};
use chrono::Utc;

use super::*;
use crate::domain::model::{EscalationStatus, Priority};
use crate::domain::service::UserEscalations;

#[derive(Default)]
struct FakeService {
    created: Mutex<Vec<(Option<i32>, String)>>,
    stored: Mutex<Vec<Escalation>>,
}

fn sample(org_id: Option<i32>) -> Escalation {
    Escalation {
        id: macro_uuid::generate_uuid_v7(),
        org_id,
        domain: "techops".to_string(),
        session_id: None,
        requester_user_id: None,
        requester_display: "Req".to_string(),
        source_channel: None,
        title: "t".to_string(),
        summary: "s".to_string(),
        tags: Vec::new(),
        priority: Priority::Normal,
        status: EscalationStatus::Open,
        assignee_user_id: None,
        assignee_team_id: None,
        callback_url: None,
        resolution: None,
        resolved_by: None,
        created_at: Utc::now(),
        claimed_at: None,
        resolved_at: None,
    }
}

impl EscalationService for FakeService {
    async fn create(
        &self,
        org_id: Option<i32>,
        request: NewEscalation,
        actor_id: &str,
    ) -> Result<Escalation> {
        self.created
            .lock()
            .unwrap()
            .push((org_id, actor_id.to_string()));
        let mut e = sample(org_id);
        e.domain = request.domain;
        self.stored.lock().unwrap().push(e.clone());
        Ok(e)
    }

    async fn get(&self, _caller: &Caller, id: Uuid) -> Result<Escalation> {
        self.stored
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .ok_or(EscalationError::NotFound)
    }

    async fn list_for_user(&self, _user_id: &str) -> Result<UserEscalations> {
        unimplemented!()
    }

    async fn list_for_team(&self, _caller: &Caller, _team_id: Uuid) -> Result<Vec<Escalation>> {
        unimplemented!()
    }

    async fn claim(&self, _caller: &Caller, _id: Uuid) -> Result<Escalation> {
        unimplemented!()
    }

    async fn reassign(
        &self,
        _caller: &Caller,
        _id: Uuid,
        _to_user: Option<String>,
        _to_team: Option<Uuid>,
        _reason: String,
    ) -> Result<Escalation> {
        unimplemented!()
    }

    async fn resolve(
        &self,
        _caller: &Caller,
        _id: Uuid,
        _resolution: String,
    ) -> Result<Escalation> {
        unimplemented!()
    }

    async fn cancel(&self, _caller: &Caller, _id: Uuid) -> Result<Escalation> {
        unimplemented!()
    }

    async fn transitions(
        &self,
        _caller: &Caller,
        _id: Uuid,
    ) -> Result<Vec<crate::domain::model::EscalationTransition>> {
        unimplemented!()
    }
}

fn agent(org_id: Option<i32>, scopes: &[&str]) -> VerifiedAgent {
    VerifiedAgent {
        principal: AgentPrincipal {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            slug: "techops".to_string(),
            display_name: "TechOps".to_string(),
            kind: AgentKind::DomainAgent,
            created_at: Utc::now(),
            disabled_at: None,
        },
        token_id: macro_uuid::generate_uuid_v7(),
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
    }
}

fn new_escalation() -> NewEscalation {
    NewEscalation {
        domain: "techops".to_string(),
        session_id: None,
        requester_user_id: None,
        requester_display: "Req".to_string(),
        source_channel: None,
        title: "t".to_string(),
        summary: "s".to_string(),
        tags: Vec::new(),
        priority: Priority::Normal,
        callback_url: None,
    }
}

#[tokio::test]
async fn create_requires_scope_and_forces_org() {
    let facade = AgentEscalationFacade::new(FakeService::default());

    let unauthorized = facade.create(&agent(Some(1), &[]), new_escalation()).await;
    assert!(matches!(
        unauthorized,
        Err(EscalationError::MissingScope { .. })
    ));

    let agent = agent(Some(7), &[SCOPE_ESCALATION_CREATE]);
    facade.create(&agent, new_escalation()).await.unwrap();
    let created = facade.service.created.lock().unwrap();
    assert_eq!(created[0].0, Some(7));
    assert_eq!(created[0].1, agent.principal.id.to_string());
}

#[tokio::test]
async fn get_enforces_org_tenancy() {
    let facade = AgentEscalationFacade::new(FakeService::default());
    let creator = agent(Some(1), &[SCOPE_ESCALATION_CREATE, SCOPE_ESCALATION_QUERY]);
    let created = facade.create(&creator, new_escalation()).await.unwrap();

    assert!(facade.get(&creator, created.id).await.is_ok());

    let other_org = agent(Some(2), &[SCOPE_ESCALATION_QUERY]);
    let hidden = facade.get(&other_org, created.id).await;
    assert!(matches!(hidden, Err(EscalationError::NotFound)));
}
