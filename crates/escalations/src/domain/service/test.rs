use std::sync::Mutex;

use super::*;
use crate::domain::model::{ExpertProfile, Priority, RoutingRule};

#[derive(Default)]
struct FakeRepo {
    escalations: Mutex<Vec<Escalation>>,
    transitions: Mutex<Vec<EscalationTransition>>,
}

impl EscalationRepo for FakeRepo {
    async fn insert(&self, escalation: &Escalation) -> Result<()> {
        self.escalations.lock().unwrap().push(escalation.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<Escalation>> {
        Ok(self
            .escalations
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.id == id)
            .cloned())
    }

    async fn list(&self, filter: &EscalationFilter) -> Result<Vec<Escalation>> {
        Ok(self
            .escalations
            .lock()
            .unwrap()
            .iter()
            .filter(|e| {
                filter
                    .assignee_user_id
                    .as_ref()
                    .is_none_or(|u| e.assignee_user_id.as_ref() == Some(u))
                    && filter
                        .assignee_team_id
                        .is_none_or(|t| e.assignee_team_id == Some(t))
                    && filter.status.is_none_or(|s| e.status == s)
                    && (!filter.unclaimed_only || e.assignee_user_id.is_none())
            })
            .cloned()
            .collect())
    }

    async fn claim(&self, id: Uuid, user_id: &str) -> Result<Option<Escalation>> {
        let mut escalations = self.escalations.lock().unwrap();
        let Some(e) = escalations.iter_mut().find(|e| e.id == id) else {
            return Ok(None);
        };
        if e.status != EscalationStatus::Open {
            return Ok(None);
        }
        e.status = EscalationStatus::Claimed;
        e.assignee_user_id = Some(user_id.to_string());
        e.claimed_at = Some(Utc::now());
        Ok(Some(e.clone()))
    }

    async fn reassign(
        &self,
        id: Uuid,
        to_user: Option<&str>,
        to_team: Option<Uuid>,
    ) -> Result<Option<Escalation>> {
        let mut escalations = self.escalations.lock().unwrap();
        let Some(e) = escalations.iter_mut().find(|e| e.id == id) else {
            return Ok(None);
        };
        e.assignee_user_id = to_user.map(str::to_string);
        e.assignee_team_id = to_team;
        e.status = if to_user.is_some() {
            EscalationStatus::Claimed
        } else {
            EscalationStatus::Open
        };
        Ok(Some(e.clone()))
    }

    async fn resolve(
        &self,
        id: Uuid,
        resolution: &str,
        resolved_by: &str,
    ) -> Result<Option<Escalation>> {
        let mut escalations = self.escalations.lock().unwrap();
        let Some(e) = escalations.iter_mut().find(|e| e.id == id) else {
            return Ok(None);
        };
        if matches!(
            e.status,
            EscalationStatus::Resolved | EscalationStatus::Cancelled
        ) {
            return Ok(None);
        }
        e.status = EscalationStatus::Resolved;
        e.resolution = Some(resolution.to_string());
        e.resolved_by = Some(resolved_by.to_string());
        e.resolved_at = Some(Utc::now());
        Ok(Some(e.clone()))
    }

    async fn cancel(&self, id: Uuid) -> Result<Option<Escalation>> {
        let mut escalations = self.escalations.lock().unwrap();
        let Some(e) = escalations.iter_mut().find(|e| e.id == id) else {
            return Ok(None);
        };
        if matches!(
            e.status,
            EscalationStatus::Resolved | EscalationStatus::Cancelled
        ) {
            return Ok(None);
        }
        e.status = EscalationStatus::Cancelled;
        e.resolved_at = Some(Utc::now());
        Ok(Some(e.clone()))
    }

    async fn insert_transition(&self, transition: &EscalationTransition) -> Result<()> {
        self.transitions.lock().unwrap().push(transition.clone());
        Ok(())
    }

    async fn list_transitions(&self, escalation_id: Uuid) -> Result<Vec<EscalationTransition>> {
        Ok(self
            .transitions
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.escalation_id == escalation_id)
            .cloned()
            .collect())
    }
}

#[derive(Default)]
struct FakeRouting {
    rules: Vec<RoutingRule>,
    experts: Vec<ExpertProfile>,
    round_robin: Mutex<Option<String>>,
}

impl RoutingRepo for FakeRouting {
    async fn list_rules(&self, _org: Option<i32>, domain: &str) -> Result<Vec<RoutingRule>> {
        Ok(self
            .rules
            .iter()
            .filter(|r| r.domain == domain)
            .cloned()
            .collect())
    }

    async fn list_all_rules(&self, _org: Option<i32>) -> Result<Vec<RoutingRule>> {
        Ok(self.rules.clone())
    }

    async fn upsert_rule(&self, _rule: &RoutingRule) -> Result<()> {
        unimplemented!()
    }

    async fn delete_rule(&self, _id: Uuid) -> Result<bool> {
        unimplemented!()
    }

    async fn get_expert(&self, _org: Option<i32>, user_id: &str) -> Result<Option<ExpertProfile>> {
        Ok(self.experts.iter().find(|e| e.user_id == user_id).cloned())
    }

    async fn list_experts(&self, _org: Option<i32>) -> Result<Vec<ExpertProfile>> {
        Ok(self.experts.clone())
    }

    async fn upsert_expert(&self, _profile: &ExpertProfile) -> Result<()> {
        unimplemented!()
    }

    async fn round_robin_last(&self, _rule_id: Uuid) -> Result<Option<String>> {
        Ok(self.round_robin.lock().unwrap().clone())
    }

    async fn set_round_robin_last(&self, _rule_id: Uuid, user_id: &str) -> Result<()> {
        *self.round_robin.lock().unwrap() = Some(user_id.to_string());
        Ok(())
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

impl EscalationCallbackClient for FakeCallback {
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

impl EscalationNotifier for FakeNotifier {
    async fn notify_assigned(&self, _e: &Escalation, recipients: &[String]) -> Result<()> {
        self.notified.lock().unwrap().push(recipients.to_vec());
        Ok(())
    }
}

type Service = EscalationServiceImpl<FakeRepo, FakeRouting, FakeTeams, FakeCallback, FakeNotifier>;

fn team_id() -> Uuid {
    Uuid::parse_str("00000000-0000-0000-0000-00000000aaaa").unwrap()
}

fn rule(target: RouteTarget) -> RoutingRule {
    RoutingRule {
        id: macro_uuid::generate_uuid_v7(),
        org_id: Some(1),
        domain: "techops".to_string(),
        position: 0,
        tags: Vec::new(),
        source_channel: None,
        min_priority: None,
        target,
    }
}

fn service_with(rules: Vec<RoutingRule>, experts: Vec<ExpertProfile>) -> Service {
    EscalationServiceImpl::new(
        FakeRepo::default(),
        FakeRouting {
            rules,
            experts,
            round_robin: Mutex::new(None),
        },
        FakeTeams {
            members: vec![
                (team_id(), "macro|a@x.com".to_string()),
                (team_id(), "macro|b@x.com".to_string()),
            ],
        },
        FakeCallback::default(),
        FakeNotifier::default(),
    )
}

fn new_escalation() -> NewEscalation {
    NewEscalation {
        domain: "techops".to_string(),
        session_id: None,
        requester_user_id: Some("macro|req@x.com".to_string()),
        requester_display: "Req".to_string(),
        source_channel: Some("slack".to_string()),
        title: "VPN down".to_string(),
        summary: "Tried X and Y".to_string(),
        tags: vec!["vpn".to_string()],
        priority: Priority::Normal,
        callback_url: Some("https://agents.internal/hooks/e".to_string()),
    }
}

#[tokio::test]
async fn routes_to_direct_user() {
    let service = service_with(
        vec![rule(RouteTarget::User {
            user_id: "macro|a@x.com".to_string(),
        })],
        Vec::new(),
    );
    let e = service
        .create(Some(1), new_escalation(), "agent-1")
        .await
        .unwrap();
    assert_eq!(e.assignee_user_id.as_deref(), Some("macro|a@x.com"));
    assert_eq!(e.status, EscalationStatus::Open);
}

#[tokio::test]
async fn routes_to_team_queue_and_claim_is_first_wins() {
    let service = service_with(
        vec![rule(RouteTarget::TeamQueue { team_id: team_id() })],
        Vec::new(),
    );
    let e = service
        .create(Some(1), new_escalation(), "agent-1")
        .await
        .unwrap();
    assert_eq!(e.assignee_team_id, Some(team_id()));
    assert!(e.assignee_user_id.is_none());

    let claimed = service
        .claim(&Caller::User("macro|a@x.com".to_string()), e.id)
        .await
        .unwrap();
    assert_eq!(claimed.status, EscalationStatus::Claimed);
    assert_eq!(claimed.assignee_user_id.as_deref(), Some("macro|a@x.com"));

    // Second claim loses.
    let second = service
        .claim(&Caller::User("macro|b@x.com".to_string()), e.id)
        .await;
    assert!(matches!(second, Err(EscalationError::Forbidden(_))));
}

#[tokio::test]
async fn claim_requires_team_membership() {
    let service = service_with(
        vec![rule(RouteTarget::TeamQueue { team_id: team_id() })],
        Vec::new(),
    );
    let e = service
        .create(Some(1), new_escalation(), "agent-1")
        .await
        .unwrap();
    let outsider = service
        .claim(&Caller::User("macro|outsider@x.com".to_string()), e.id)
        .await;
    assert!(matches!(outsider, Err(EscalationError::Forbidden(_))));
}

#[tokio::test]
async fn round_robin_rotates_and_skips_unavailable() {
    let rr = rule(RouteTarget::TeamRoundRobin { team_id: team_id() });
    let service = service_with(
        vec![rr],
        vec![ExpertProfile {
            user_id: "macro|b@x.com".to_string(),
            org_id: Some(1),
            domains: vec!["techops".to_string()],
            tags: Vec::new(),
            available: false,
        }],
    );
    // b is unavailable, so both picks land on a.
    let first = service
        .create(Some(1), new_escalation(), "agent-1")
        .await
        .unwrap();
    assert_eq!(first.assignee_user_id.as_deref(), Some("macro|a@x.com"));
    let second = service
        .create(Some(1), new_escalation(), "agent-1")
        .await
        .unwrap();
    assert_eq!(second.assignee_user_id.as_deref(), Some("macro|a@x.com"));
}

#[tokio::test]
async fn round_robin_alternates_between_available_members() {
    let rr = rule(RouteTarget::TeamRoundRobin { team_id: team_id() });
    let service = service_with(vec![rr], Vec::new());
    let first = service
        .create(Some(1), new_escalation(), "agent-1")
        .await
        .unwrap();
    let second = service
        .create(Some(1), new_escalation(), "agent-1")
        .await
        .unwrap();
    assert_ne!(first.assignee_user_id, second.assignee_user_id);
}

#[tokio::test]
async fn unmatched_request_stays_unassigned() {
    let service = service_with(Vec::new(), Vec::new());
    let e = service
        .create(Some(1), new_escalation(), "agent-1")
        .await
        .unwrap();
    assert!(e.assignee_user_id.is_none());
    assert!(e.assignee_team_id.is_none());
    assert_eq!(e.status, EscalationStatus::Open);
}

#[tokio::test]
async fn reassign_requires_reason_and_records_transition() {
    let service = service_with(
        vec![rule(RouteTarget::TeamQueue { team_id: team_id() })],
        Vec::new(),
    );
    let e = service
        .create(Some(1), new_escalation(), "agent-1")
        .await
        .unwrap();

    let no_reason = service
        .reassign(
            &Caller::User("macro|a@x.com".to_string()),
            e.id,
            Some("macro|b@x.com".to_string()),
            None,
            "  ".to_string(),
        )
        .await;
    assert!(matches!(no_reason, Err(EscalationError::InvalidRequest(_))));

    let reassigned = service
        .reassign(
            &Caller::User("macro|a@x.com".to_string()),
            e.id,
            Some("macro|b@x.com".to_string()),
            None,
            "belongs to b".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(
        reassigned.assignee_user_id.as_deref(),
        Some("macro|b@x.com")
    );

    let transitions = service.transitions(&Caller::Internal, e.id).await.unwrap();
    let reassign = transitions
        .iter()
        .find(|t| t.action == "reassigned")
        .unwrap();
    assert_eq!(reassign.reason.as_deref(), Some("belongs to b"));
}

#[tokio::test]
async fn resolve_fires_callback_and_is_terminal() {
    let service = service_with(
        vec![rule(RouteTarget::User {
            user_id: "macro|a@x.com".to_string(),
        })],
        Vec::new(),
    );
    let e = service
        .create(Some(1), new_escalation(), "agent-1")
        .await
        .unwrap();
    let resolved = service
        .resolve(
            &Caller::User("macro|a@x.com".to_string()),
            e.id,
            "Restart the VPN concentrator".to_string(),
        )
        .await
        .unwrap();
    assert_eq!(resolved.status, EscalationStatus::Resolved);

    let deliveries = service.callback.deliveries.lock().unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].0, "https://agents.internal/hooks/e");
    assert_eq!(
        deliveries[0].1["resolution"],
        "Restart the VPN concentrator"
    );
    drop(deliveries);

    let again = service
        .resolve(
            &Caller::User("macro|a@x.com".to_string()),
            e.id,
            "again".to_string(),
        )
        .await;
    assert!(matches!(again, Err(EscalationError::InvalidStatus(_))));
}

#[tokio::test]
async fn list_for_user_splits_assigned_and_claimable() {
    let service = service_with(
        vec![rule(RouteTarget::TeamQueue { team_id: team_id() })],
        Vec::new(),
    );
    let queued = service
        .create(Some(1), new_escalation(), "agent-1")
        .await
        .unwrap();
    let mine = service
        .create(Some(1), new_escalation(), "agent-1")
        .await
        .unwrap();
    service
        .claim(&Caller::User("macro|a@x.com".to_string()), mine.id)
        .await
        .unwrap();

    let view = service.list_for_user("macro|a@x.com").await.unwrap();
    assert_eq!(view.assigned.len(), 1);
    assert_eq!(view.assigned[0].id, mine.id);
    assert_eq!(view.claimable.len(), 1);
    assert_eq!(view.claimable[0].id, queued.id);
}
