use super::*;
use crate::domain::model::{InboxError, InboxKind, Result};
use crate::domain::ports::{ApprovalInboxSource, EscalationInboxSource, SkillProposalInboxSource};
use approvals::domain::model::{ApprovalRequest, ApprovalStatus};
use approvals::domain::service::UserApprovals;
use chrono::{Duration, Utc};
use escalations::domain::model::{Escalation, EscalationError, EscalationStatus, Priority};
use escalations::domain::service::UserEscalations;
use skill_governance::domain::model::{ProposalKind, ProposalStatus, SkillProposal, SkillScope};
use skill_governance::domain::service::UserProposals;

struct FakeEscalations {
    view: UserEscalations,
}

impl EscalationInboxSource for FakeEscalations {
    async fn list_for_user(&self, _user_id: &str) -> Result<UserEscalations> {
        Ok(self.view.clone())
    }
}

struct FailingEscalations;

impl EscalationInboxSource for FailingEscalations {
    async fn list_for_user(&self, _user_id: &str) -> Result<UserEscalations> {
        Err(InboxError::Escalation(EscalationError::NotFound))
    }
}

struct FakeApprovals {
    view: UserApprovals,
}

impl ApprovalInboxSource for FakeApprovals {
    async fn list_for_user(&self, _user_id: &str) -> Result<UserApprovals> {
        Ok(self.view.clone())
    }
}

struct FakeProposals {
    view: UserProposals,
}

impl SkillProposalInboxSource for FakeProposals {
    async fn list_for_user(&self, _user_id: &str) -> Result<UserProposals> {
        Ok(self.view.clone())
    }
}

fn empty_sources() -> InboxServiceImpl<FakeEscalations, FakeApprovals, FakeProposals> {
    InboxServiceImpl::new(
        FakeEscalations {
            view: UserEscalations {
                assigned: vec![],
                claimable: vec![],
            },
        },
        FakeApprovals {
            view: UserApprovals {
                assigned: vec![],
                team_queue: vec![],
            },
        },
        FakeProposals {
            view: UserProposals {
                assigned: vec![],
                team_queue: vec![],
            },
        },
    )
}

fn escalation(title: &str, minutes_ago: i64, status: EscalationStatus) -> Escalation {
    Escalation {
        id: macro_uuid::generate_uuid_v7(),
        org_id: Some(1),
        domain: "techops".into(),
        session_id: None,
        requester_user_id: None,
        requester_display: "Req".into(),
        source_channel: None,
        title: title.into(),
        summary: "briefing".into(),
        tags: vec![],
        priority: Priority::Normal,
        status,
        assignee_user_id: None,
        assignee_team_id: None,
        callback_url: None,
        resolution: None,
        resolved_by: None,
        created_at: Utc::now() - Duration::minutes(minutes_ago),
        claimed_at: None,
        resolved_at: None,
    }
}

fn approval(summary: &str, minutes_ago: i64) -> ApprovalRequest {
    ApprovalRequest {
        id: macro_uuid::generate_uuid_v7(),
        org_id: Some(1),
        agent_slug: "techops".into(),
        session_id: None,
        requester_user_id: None,
        requester_display: "Req".into(),
        tool_name: "send_email".into(),
        arguments: serde_json::json!({}),
        arguments_digest: "abc".into(),
        summary: summary.into(),
        status: ApprovalStatus::Pending,
        assignee_user_id: None,
        assignee_team_id: None,
        callback_url: None,
        decided_by: None,
        decision_note: None,
        created_at: Utc::now() - Duration::minutes(minutes_ago),
        decided_at: None,
        consumed_at: None,
    }
}

fn proposal(name: &str, minutes_ago: i64) -> SkillProposal {
    let now = Utc::now();
    SkillProposal {
        id: macro_uuid::generate_uuid_v7(),
        org_id: Some(1),
        skill_id: None,
        kind: ProposalKind::Create,
        slug: "slug".into(),
        target_scope: SkillScope::Org,
        owner_user_id: None,
        owner_team_id: None,
        proposed_name: name.into(),
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
        created_at: now - Duration::minutes(minutes_ago),
        updated_at: now,
    }
}

#[tokio::test]
async fn empty_sources_yield_empty_inbox() {
    let view = empty_sources().list_mine("user-1").await.unwrap();
    assert!(view.items.is_empty());
}

#[tokio::test]
async fn merges_three_sources_newest_first_with_locked_fields() {
    let assigned_esc = escalation("VPN down", 10, EscalationStatus::Claimed);
    let queued_esc = escalation("SSO stuck", 3, EscalationStatus::Open);
    let assigned_appr = approval("send the invoice", 7);
    let queued_appr = approval("delete the draft", 1);
    let assigned_prop = proposal("Onboarding playbook", 20);
    let queued_prop = proposal("Incident runbook", 5);

    let esc_assigned_id = assigned_esc.id;
    let esc_queued_id = queued_esc.id;
    let appr_assigned_id = assigned_appr.id;
    let appr_queued_id = queued_appr.id;
    let prop_assigned_id = assigned_prop.id;
    let prop_queued_id = queued_prop.id;

    let svc = InboxServiceImpl::new(
        FakeEscalations {
            view: UserEscalations {
                assigned: vec![assigned_esc],
                claimable: vec![queued_esc],
            },
        },
        FakeApprovals {
            view: UserApprovals {
                assigned: vec![assigned_appr],
                team_queue: vec![queued_appr],
            },
        },
        FakeProposals {
            view: UserProposals {
                assigned: vec![assigned_prop],
                team_queue: vec![queued_prop],
            },
        },
    );

    let view = svc.list_mine("user-1").await.unwrap();
    assert_eq!(view.items.len(), 6);
    let titles: Vec<_> = view.items.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "delete the draft",
            "SSO stuck",
            "Incident runbook",
            "send the invoice",
            "VPN down",
            "Onboarding playbook",
        ]
    );

    let vpn = view.items.iter().find(|i| i.id == esc_assigned_id).unwrap();
    assert_eq!(vpn.kind, InboxKind::Escalation);
    assert_eq!(vpn.status, "claimed");
    assert_eq!(vpn.href, format!("/escalations/{esc_assigned_id}"));
    assert!(vpn.assigned_to_me);
    assert!(!vpn.team_queued);

    let sso = view.items.iter().find(|i| i.id == esc_queued_id).unwrap();
    assert_eq!(sso.href, format!("/escalations/{esc_queued_id}"));
    assert!(!sso.assigned_to_me);
    assert!(sso.team_queued);

    let invoice = view
        .items
        .iter()
        .find(|i| i.id == appr_assigned_id)
        .unwrap();
    assert_eq!(invoice.kind, InboxKind::Approval);
    assert_eq!(invoice.status, "pending");
    assert_eq!(invoice.href, format!("/approvals/{appr_assigned_id}"));
    assert!(invoice.assigned_to_me);
    assert!(!invoice.team_queued);

    let draft = view.items.iter().find(|i| i.id == appr_queued_id).unwrap();
    assert_eq!(draft.href, format!("/approvals/{appr_queued_id}"));
    assert!(!draft.assigned_to_me);
    assert!(draft.team_queued);

    let playbook = view
        .items
        .iter()
        .find(|i| i.id == prop_assigned_id)
        .unwrap();
    assert_eq!(playbook.kind, InboxKind::SkillProposal);
    assert_eq!(playbook.status, "pending");
    assert_eq!(
        playbook.href,
        format!("/skill-proposals/{prop_assigned_id}")
    );
    assert!(playbook.assigned_to_me);
    assert!(!playbook.team_queued);

    let runbook = view.items.iter().find(|i| i.id == prop_queued_id).unwrap();
    assert_eq!(runbook.href, format!("/skill-proposals/{prop_queued_id}"));
    assert!(!runbook.assigned_to_me);
    assert!(runbook.team_queued);
}

#[tokio::test]
async fn source_error_fails_the_inbox() {
    let svc = InboxServiceImpl::new(
        FailingEscalations,
        FakeApprovals {
            view: UserApprovals {
                assigned: vec![],
                team_queue: vec![],
            },
        },
        FakeProposals {
            view: UserProposals {
                assigned: vec![],
                team_queue: vec![],
            },
        },
    );
    let err = svc.list_mine("user-1").await.unwrap_err();
    assert!(matches!(
        err,
        InboxError::Escalation(EscalationError::NotFound)
    ));
}
