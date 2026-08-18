use std::sync::Mutex;

use agent_identity::domain::model::{AgentKind, AgentPrincipal, VerifiedAgent};
use chrono::Utc;

use super::*;
use crate::domain::model::ApprovalTransition;
use crate::domain::service::UserApprovals;

#[derive(Default)]
struct FakeService {
    gated: Mutex<Vec<(Option<i32>, String, String)>>,
    stored: Mutex<Vec<ApprovalRequest>>,
    cancelled: Mutex<Vec<Uuid>>,
}

fn sample(org_id: Option<i32>, agent_slug: &str) -> ApprovalRequest {
    ApprovalRequest {
        id: macro_uuid::generate_uuid_v7(),
        org_id,
        agent_slug: agent_slug.to_string(),
        session_id: None,
        requester_user_id: None,
        requester_display: "Req".to_string(),
        tool_name: "send_email".to_string(),
        arguments: serde_json::json!({}),
        arguments_digest: "d".to_string(),
        summary: "s".to_string(),
        status: ApprovalStatus::Pending,
        assignee_user_id: None,
        assignee_team_id: None,
        callback_url: None,
        decided_by: None,
        decision_note: None,
        created_at: Utc::now(),
        decided_at: None,
        consumed_at: None,
    }
}

impl ApprovalService for FakeService {
    async fn evaluate(
        &self,
        _org_id: Option<i32>,
        _agent_slug: &str,
        _tool_name: &str,
    ) -> Result<crate::domain::model::PolicyDecision> {
        unimplemented!()
    }

    async fn gate(
        &self,
        org_id: Option<i32>,
        request: GateRequest,
        actor_id: &str,
    ) -> Result<GateOutcome> {
        self.gated
            .lock()
            .unwrap()
            .push((org_id, request.agent_slug.clone(), actor_id.to_string()));
        let stored = sample(org_id, &request.agent_slug);
        self.stored.lock().unwrap().push(stored.clone());
        Ok(GateOutcome::Pending {
            request: Box::new(stored),
        })
    }

    async fn get(&self, _caller: &Caller, id: Uuid) -> Result<ApprovalRequest> {
        self.stored
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or(ApprovalError::NotFound)
    }

    async fn list_for_user(&self, _user_id: &str) -> Result<UserApprovals> {
        unimplemented!()
    }

    async fn list_for_team(
        &self,
        _caller: &Caller,
        _team_id: Uuid,
    ) -> Result<Vec<ApprovalRequest>> {
        unimplemented!()
    }

    async fn decide(
        &self,
        _caller: &Caller,
        _id: Uuid,
        _approved: bool,
        _note: Option<String>,
    ) -> Result<ApprovalRequest> {
        unimplemented!()
    }

    async fn reassign(
        &self,
        _caller: &Caller,
        _id: Uuid,
        _to_user: Option<String>,
        _to_team: Option<Uuid>,
        _reason: String,
    ) -> Result<ApprovalRequest> {
        unimplemented!()
    }

    async fn cancel(&self, _caller: &Caller, id: Uuid) -> Result<ApprovalRequest> {
        self.cancelled.lock().unwrap().push(id);
        let mut stored = self.stored.lock().unwrap();
        let r = stored
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or(ApprovalError::NotFound)?;
        r.status = ApprovalStatus::Cancelled;
        Ok(r.clone())
    }

    async fn transitions(&self, _caller: &Caller, _id: Uuid) -> Result<Vec<ApprovalTransition>> {
        unimplemented!()
    }
}

fn agent(org_id: Option<i32>, slug: &str, scopes: &[&str]) -> VerifiedAgent {
    VerifiedAgent {
        principal: AgentPrincipal {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            slug: slug.to_string(),
            display_name: "TechOps".to_string(),
            kind: AgentKind::DomainAgent,
            created_at: Utc::now(),
            disabled_at: None,
        },
        token_id: macro_uuid::generate_uuid_v7(),
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
    }
}

fn gate_request() -> GateRequest {
    GateRequest {
        agent_slug: "spoofed".to_string(),
        session_id: None,
        requester_user_id: None,
        requester_display: "Req".to_string(),
        tool_name: "send_email".to_string(),
        arguments: serde_json::json!({}),
        summary: "s".to_string(),
        callback_url: None,
    }
}

#[tokio::test]
async fn gate_requires_scope_and_forces_org_and_slug() {
    let facade = AgentApprovalFacade::new(FakeService::default());

    let unauthorized = facade
        .gate(&agent(Some(1), "techops", &[]), gate_request())
        .await;
    assert!(matches!(
        unauthorized,
        Err(ApprovalError::MissingScope { .. })
    ));

    let caller = agent(Some(7), "techops", &[SCOPE_APPROVAL_GATE]);
    facade.gate(&caller, gate_request()).await.unwrap();
    let gated = facade.service.gated.lock().unwrap();
    assert_eq!(gated[0].0, Some(7));
    // The spoofed slug in the body is overwritten by the principal's slug.
    assert_eq!(gated[0].1, "techops");
    assert_eq!(gated[0].2, caller.principal.id.to_string());
}

#[tokio::test]
async fn get_enforces_org_tenancy() {
    let facade = AgentApprovalFacade::new(FakeService::default());
    let creator = agent(
        Some(1),
        "techops",
        &[SCOPE_APPROVAL_GATE, SCOPE_APPROVAL_QUERY],
    );
    let GateOutcome::Pending { request } = facade.gate(&creator, gate_request()).await.unwrap()
    else {
        panic!("expected pending");
    };

    assert!(facade.get(&creator, request.id).await.is_ok());

    let other_org = agent(Some(2), "techops", &[SCOPE_APPROVAL_QUERY]);
    let hidden = facade.get(&other_org, request.id).await;
    assert!(matches!(hidden, Err(ApprovalError::NotFound)));
}

#[tokio::test]
async fn cancel_is_limited_to_own_pending_requests() {
    let facade = AgentApprovalFacade::new(FakeService::default());
    let creator = agent(Some(1), "techops", &[SCOPE_APPROVAL_GATE]);
    let GateOutcome::Pending { request } = facade.gate(&creator, gate_request()).await.unwrap()
    else {
        panic!("expected pending");
    };

    // A different agent principal (same org) cannot cancel it.
    let other_agent = agent(Some(1), "security", &[SCOPE_APPROVAL_GATE]);
    let denied = facade.cancel(&other_agent, request.id).await;
    assert!(matches!(denied, Err(ApprovalError::NotFound)));

    let cancelled = facade.cancel(&creator, request.id).await.unwrap();
    assert_eq!(cancelled.status, ApprovalStatus::Cancelled);

    // Already terminal now.
    let again = facade.cancel(&creator, request.id).await;
    assert!(matches!(again, Err(ApprovalError::InvalidStatus(_))));
}
