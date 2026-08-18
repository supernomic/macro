use std::sync::Mutex;

use super::*;
use crate::domain::model::{FloorEntry, PolicyFloor};

#[derive(Default)]
struct FakeRepo {
    requests: Mutex<Vec<ApprovalRequest>>,
    transitions: Mutex<Vec<ApprovalTransition>>,
}

impl ApprovalRepo for FakeRepo {
    async fn insert(&self, request: &ApprovalRequest) -> Result<()> {
        self.requests.lock().unwrap().push(request.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<ApprovalRequest>> {
        Ok(self
            .requests
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.id == id)
            .cloned())
    }

    async fn list(&self, filter: &ApprovalFilter) -> Result<Vec<ApprovalRequest>> {
        Ok(self
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| {
                filter
                    .assignee_user_id
                    .as_ref()
                    .is_none_or(|u| r.assignee_user_id.as_ref() == Some(u))
                    && filter
                        .assignee_team_id
                        .is_none_or(|t| r.assignee_team_id == Some(t))
                    && filter.status.is_none_or(|s| r.status == s)
                    && (!filter.unassigned_only || r.assignee_user_id.is_none())
            })
            .cloned()
            .collect())
    }

    async fn find_latest_for_gate(
        &self,
        session_id: Option<Uuid>,
        tool_name: &str,
        arguments_digest: &str,
    ) -> Result<Option<ApprovalRequest>> {
        Ok(self
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| {
                r.session_id == session_id
                    && r.tool_name == tool_name
                    && r.arguments_digest == arguments_digest
            })
            .max_by_key(|r| r.created_at)
            .cloned())
    }

    async fn decide(
        &self,
        id: Uuid,
        approved: bool,
        decided_by: &str,
        note: Option<&str>,
    ) -> Result<Option<ApprovalRequest>> {
        let mut requests = self.requests.lock().unwrap();
        let Some(r) = requests.iter_mut().find(|r| r.id == id) else {
            return Ok(None);
        };
        if r.status != ApprovalStatus::Pending {
            return Ok(None);
        }
        r.status = if approved {
            ApprovalStatus::Approved
        } else {
            ApprovalStatus::Denied
        };
        r.decided_by = Some(decided_by.to_string());
        r.decision_note = note.map(str::to_string);
        r.decided_at = Some(Utc::now());
        Ok(Some(r.clone()))
    }

    async fn consume(&self, id: Uuid) -> Result<Option<ApprovalRequest>> {
        let mut requests = self.requests.lock().unwrap();
        let Some(r) = requests.iter_mut().find(|r| r.id == id) else {
            return Ok(None);
        };
        if r.consumed_at.is_some()
            || !matches!(r.status, ApprovalStatus::Approved | ApprovalStatus::Denied)
        {
            return Ok(None);
        }
        r.consumed_at = Some(Utc::now());
        Ok(Some(r.clone()))
    }

    async fn reassign(
        &self,
        id: Uuid,
        to_user: Option<&str>,
        to_team: Option<Uuid>,
    ) -> Result<Option<ApprovalRequest>> {
        let mut requests = self.requests.lock().unwrap();
        let Some(r) = requests.iter_mut().find(|r| r.id == id) else {
            return Ok(None);
        };
        r.assignee_user_id = to_user.map(str::to_string);
        r.assignee_team_id = to_team;
        Ok(Some(r.clone()))
    }

    async fn cancel(&self, id: Uuid) -> Result<Option<ApprovalRequest>> {
        let mut requests = self.requests.lock().unwrap();
        let Some(r) = requests.iter_mut().find(|r| r.id == id) else {
            return Ok(None);
        };
        if r.status != ApprovalStatus::Pending {
            return Ok(None);
        }
        r.status = ApprovalStatus::Cancelled;
        r.decided_at = Some(Utc::now());
        Ok(Some(r.clone()))
    }

    async fn insert_transition(&self, transition: &ApprovalTransition) -> Result<()> {
        self.transitions.lock().unwrap().push(transition.clone());
        Ok(())
    }

    async fn list_transitions(&self, approval_id: Uuid) -> Result<Vec<ApprovalTransition>> {
        Ok(self
            .transitions
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.approval_id == approval_id)
            .cloned()
            .collect())
    }
}

struct FakePolicies {
    policies: Vec<ToolPolicy>,
}

impl PolicyRepo for FakePolicies {
    async fn list_policies(&self, _org: Option<i32>) -> Result<Vec<ToolPolicy>> {
        Ok(self.policies.clone())
    }

    async fn upsert_policy(&self, _policy: &ToolPolicy) -> Result<()> {
        unimplemented!()
    }

    async fn delete_policy(&self, _id: Uuid) -> Result<bool> {
        unimplemented!()
    }
}

struct FakeTeams {
    members: Vec<(Uuid, String)>,
}

impl TeamMembershipPort for FakeTeams {
    async fn is_member(&self, user_id: &str, team_id: Uuid) -> Result<bool> {
        Ok(self
            .members
            .iter()
            .any(|(t, u)| *t == team_id && u == user_id))
    }

    async fn team_members(&self, team_id: Uuid) -> Result<Vec<String>> {
        Ok(self
            .members
            .iter()
            .filter(|(t, _)| *t == team_id)
            .map(|(_, u)| u.clone())
            .collect())
    }

    async fn user_teams(&self, user_id: &str) -> Result<Vec<Uuid>> {
        Ok(self
            .members
            .iter()
            .filter(|(_, u)| u == user_id)
            .map(|(t, _)| *t)
            .collect())
    }
}

#[derive(Default)]
struct FakeCallback {
    deliveries: Mutex<Vec<(String, serde_json::Value)>>,
}

impl ApprovalCallbackClient for FakeCallback {
    async fn deliver(&self, url: &str, payload: &serde_json::Value) -> Result<()> {
        self.deliveries
            .lock()
            .unwrap()
            .push((url.to_string(), payload.clone()));
        Ok(())
    }
}

#[derive(Default)]
struct FakeNotifier {
    notified: Mutex<Vec<Vec<String>>>,
}

impl ApprovalNotifier for FakeNotifier {
    async fn notify_assigned(&self, _r: &ApprovalRequest, recipients: &[String]) -> Result<()> {
        self.notified.lock().unwrap().push(recipients.to_vec());
        Ok(())
    }
}

type Service = ApprovalServiceImpl<FakeRepo, FakePolicies, FakeTeams, FakeCallback, FakeNotifier>;

fn team_id() -> Uuid {
    Uuid::parse_str("00000000-0000-0000-0000-00000000bbbb").unwrap()
}

fn policy(
    agent_slug: &str,
    tool_name: &str,
    decision: PolicyDecision,
    approver_user: Option<&str>,
) -> ToolPolicy {
    ToolPolicy {
        id: macro_uuid::generate_uuid_v7(),
        org_id: Some(1),
        agent_slug: agent_slug.to_string(),
        tool_name: tool_name.to_string(),
        decision,
        approver_user_id: approver_user.map(str::to_string),
        approver_team_id: None,
    }
}

fn service_with(policies: Vec<ToolPolicy>, floor: PolicyFloor) -> Service {
    ApprovalServiceImpl::new(
        FakeRepo::default(),
        FakePolicies { policies },
        FakeTeams {
            members: vec![
                (team_id(), "macro|a@x.com".to_string()),
                (team_id(), "macro|b@x.com".to_string()),
            ],
        },
        FakeCallback::default(),
        FakeNotifier::default(),
        floor,
    )
}

fn gate_request(tool: &str) -> GateRequest {
    GateRequest {
        agent_slug: "techops".to_string(),
        session_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000111").unwrap()),
        requester_user_id: Some("macro|req@x.com".to_string()),
        requester_display: "Req".to_string(),
        tool_name: tool.to_string(),
        arguments: serde_json::json!({"to": "cfo@x.com"}),
        summary: "Send the renewal email".to_string(),
        callback_url: Some("https://agents.internal/hooks/a".to_string()),
    }
}

#[tokio::test]
async fn builtin_floor_requires_approval_for_send_and_cannot_be_loosened() {
    let floor = PolicyFloor::builtin();
    let service = service_with(
        vec![policy("techops", "send_email", PolicyDecision::Allow, None)],
        floor,
    );
    let outcome = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap();
    assert!(matches!(outcome, GateOutcome::Pending { .. }));

    // Search stays allow.
    let search = service
        .gate(Some(1), gate_request("search_documents"), "agent-1")
        .await
        .unwrap();
    assert!(matches!(search, GateOutcome::Allow));
}

#[tokio::test]
async fn allow_by_default_when_no_policy_matches() {
    let service = service_with(Vec::new(), PolicyFloor::default());
    let outcome = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap();
    assert!(matches!(outcome, GateOutcome::Allow));
}

#[tokio::test]
async fn deny_policy_refuses() {
    let service = service_with(
        vec![policy("*", "send_email", PolicyDecision::Deny, None)],
        PolicyFloor::default(),
    );
    let outcome = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap();
    assert!(matches!(outcome, GateOutcome::Deny { .. }));
}

#[tokio::test]
async fn floor_cannot_be_loosened_by_org_allow() {
    let floor = PolicyFloor {
        entries: vec![FloorEntry {
            agent_slug: "*".to_string(),
            tool_name: "send_email".to_string(),
            decision: PolicyDecision::RequireApproval,
        }],
    };
    // The org tries to allow it outright; the floor still requires
    // approval.
    let service = service_with(
        vec![policy("techops", "send_email", PolicyDecision::Allow, None)],
        floor,
    );
    let outcome = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap();
    assert!(matches!(outcome, GateOutcome::Pending { .. }));
}

#[tokio::test]
async fn org_can_tighten_floor() {
    let floor = PolicyFloor {
        entries: vec![FloorEntry {
            agent_slug: "*".to_string(),
            tool_name: "send_email".to_string(),
            decision: PolicyDecision::RequireApproval,
        }],
    };
    let service = service_with(
        vec![policy("techops", "send_email", PolicyDecision::Deny, None)],
        floor,
    );
    let outcome = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap();
    assert!(matches!(outcome, GateOutcome::Deny { .. }));
}

#[tokio::test]
async fn most_specific_policy_wins() {
    let service = service_with(
        vec![
            policy("*", "*", PolicyDecision::Deny, None),
            policy("techops", "send_email", PolicyDecision::Allow, None),
        ],
        PolicyFloor::default(),
    );
    let outcome = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap();
    assert!(matches!(outcome, GateOutcome::Allow));
    // A different tool still hits the broad deny.
    let other = service
        .gate(Some(1), gate_request("delete_document"), "agent-1")
        .await
        .unwrap();
    assert!(matches!(other, GateOutcome::Deny { .. }));
}

#[tokio::test]
async fn require_approval_routes_to_policy_approver() {
    let service = service_with(
        vec![policy(
            "techops",
            "send_email",
            PolicyDecision::RequireApproval,
            Some("macro|a@x.com"),
        )],
        PolicyFloor::default(),
    );
    let outcome = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap();
    let GateOutcome::Pending { request } = outcome else {
        panic!("expected pending");
    };
    assert_eq!(request.status, ApprovalStatus::Pending);
    assert_eq!(request.assignee_user_id.as_deref(), Some("macro|a@x.com"));
}

#[tokio::test]
async fn approved_request_authorizes_exactly_one_retry() {
    let service = service_with(
        vec![policy(
            "techops",
            "send_email",
            PolicyDecision::RequireApproval,
            Some("macro|a@x.com"),
        )],
        PolicyFloor::default(),
    );
    let GateOutcome::Pending { request } = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap()
    else {
        panic!("expected pending");
    };

    // Re-gating while pending returns the same open request.
    let GateOutcome::Pending { request: same } = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap()
    else {
        panic!("expected pending");
    };
    assert_eq!(same.id, request.id);

    service
        .decide(
            &Caller::User("macro|a@x.com".to_string()),
            request.id,
            true,
            None,
        )
        .await
        .unwrap();

    // First retry consumes the approval.
    let retry = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap();
    assert!(matches!(retry, GateOutcome::Allow));

    // Second retry needs a fresh approval.
    let again = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap();
    let GateOutcome::Pending { request: fresh } = again else {
        panic!("expected a fresh pending request");
    };
    assert_ne!(fresh.id, request.id);
}

#[tokio::test]
async fn denied_request_refuses_the_retry_with_note() {
    let service = service_with(
        vec![policy(
            "techops",
            "send_email",
            PolicyDecision::RequireApproval,
            Some("macro|a@x.com"),
        )],
        PolicyFloor::default(),
    );
    let GateOutcome::Pending { request } = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap()
    else {
        panic!("expected pending");
    };
    service
        .decide(
            &Caller::User("macro|a@x.com".to_string()),
            request.id,
            false,
            Some("wrong recipient".to_string()),
        )
        .await
        .unwrap();

    let retry = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap();
    let GateOutcome::Deny { reason } = retry else {
        panic!("expected deny");
    };
    assert_eq!(reason, "wrong recipient");
}

#[tokio::test]
async fn decide_fires_callback_and_requires_assignment() {
    let service = service_with(
        vec![policy(
            "techops",
            "send_email",
            PolicyDecision::RequireApproval,
            Some("macro|a@x.com"),
        )],
        PolicyFloor::default(),
    );
    let GateOutcome::Pending { request } = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap()
    else {
        panic!("expected pending");
    };

    // An outsider cannot decide.
    let outsider = service
        .decide(
            &Caller::User("macro|outsider@x.com".to_string()),
            request.id,
            true,
            None,
        )
        .await;
    assert!(matches!(outsider, Err(ApprovalError::Forbidden(_))));

    let decided = service
        .decide(
            &Caller::User("macro|a@x.com".to_string()),
            request.id,
            true,
            Some("go ahead".to_string()),
        )
        .await
        .unwrap();
    assert_eq!(decided.status, ApprovalStatus::Approved);

    {
        let deliveries = service.callback.deliveries.lock().unwrap();
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].0, "https://agents.internal/hooks/a");
        assert_eq!(deliveries[0].1["approved"], true);
    }

    // Deciding twice conflicts.
    let again = service
        .decide(
            &Caller::User("macro|a@x.com".to_string()),
            request.id,
            false,
            None,
        )
        .await;
    assert!(matches!(again, Err(ApprovalError::InvalidStatus(_))));
}

#[tokio::test]
async fn team_member_can_decide_team_routed_request() {
    let mut p = policy(
        "techops",
        "send_email",
        PolicyDecision::RequireApproval,
        None,
    );
    p.approver_team_id = Some(team_id());
    let service = service_with(vec![p], PolicyFloor::default());
    let GateOutcome::Pending { request } = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap()
    else {
        panic!("expected pending");
    };
    assert_eq!(request.assignee_team_id, Some(team_id()));

    let decided = service
        .decide(
            &Caller::User("macro|b@x.com".to_string()),
            request.id,
            true,
            None,
        )
        .await
        .unwrap();
    assert_eq!(decided.decided_by.as_deref(), Some("macro|b@x.com"));
}

#[tokio::test]
async fn reassign_requires_reason_and_records_transition() {
    let mut p = policy(
        "techops",
        "send_email",
        PolicyDecision::RequireApproval,
        None,
    );
    p.approver_team_id = Some(team_id());
    let service = service_with(vec![p], PolicyFloor::default());
    let GateOutcome::Pending { request } = service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap()
    else {
        panic!("expected pending");
    };

    let no_reason = service
        .reassign(
            &Caller::User("macro|a@x.com".to_string()),
            request.id,
            Some("macro|b@x.com".to_string()),
            None,
            " ".to_string(),
        )
        .await;
    assert!(matches!(no_reason, Err(ApprovalError::InvalidRequest(_))));

    let reassigned = service
        .reassign(
            &Caller::User("macro|a@x.com".to_string()),
            request.id,
            Some("macro|b@x.com".to_string()),
            None,
            "this belongs to b".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(
        reassigned.assignee_user_id.as_deref(),
        Some("macro|b@x.com")
    );

    let transitions = service
        .transitions(&Caller::Internal, request.id)
        .await
        .unwrap();
    let t = transitions
        .iter()
        .find(|t| t.action == "reassigned")
        .unwrap();
    assert_eq!(t.reason.as_deref(), Some("this belongs to b"));
}

#[tokio::test]
async fn list_for_user_splits_assigned_and_team_queue() {
    let mut team_policy = policy(
        "techops",
        "send_email",
        PolicyDecision::RequireApproval,
        None,
    );
    team_policy.approver_team_id = Some(team_id());
    let direct_policy = policy(
        "techops",
        "delete_document",
        PolicyDecision::RequireApproval,
        Some("macro|a@x.com"),
    );
    let service = service_with(vec![team_policy, direct_policy], PolicyFloor::default());

    service
        .gate(Some(1), gate_request("send_email"), "agent-1")
        .await
        .unwrap();
    service
        .gate(Some(1), gate_request("delete_document"), "agent-1")
        .await
        .unwrap();

    let view = service.list_for_user("macro|a@x.com").await.unwrap();
    assert_eq!(view.assigned.len(), 1);
    assert_eq!(view.assigned[0].tool_name, "delete_document");
    assert_eq!(view.team_queue.len(), 1);
    assert_eq!(view.team_queue[0].tool_name, "send_email");
}
