//! Aggregating inbox service: reuse each source's `list_for_user`.

#[cfg(test)]
mod test;

use super::model::{InboxItem, InboxKind, InboxView, Result};
use super::ports::{ApprovalInboxSource, EscalationInboxSource, SkillProposalInboxSource};
use approvals::domain::model::ApprovalRequest;
use approvals::domain::service::UserApprovals;
use escalations::domain::model::Escalation;
use escalations::domain::service::UserEscalations;
use skill_governance::domain::model::SkillProposal;
use skill_governance::domain::service::UserProposals;

/// Domain service exposed to the inbox inbound adapter.
pub trait InboxService: Send + Sync + 'static {
    /// Unified personal inbox for `user_id`, newest first.
    fn list_mine(&self, user_id: &str) -> impl Future<Output = Result<InboxView>> + Send;
}

/// Facade over the three list sources.
#[derive(Debug, Clone)]
pub struct InboxServiceImpl<E, A, S> {
    escalations: E,
    approvals: A,
    skills: S,
}

impl<E, A, S> InboxServiceImpl<E, A, S> {
    /// Compose the three personal-inbox sources.
    pub fn new(escalations: E, approvals: A, skills: S) -> Self {
        Self {
            escalations,
            approvals,
            skills,
        }
    }
}

impl<E, A, S> InboxService for InboxServiceImpl<E, A, S>
where
    E: EscalationInboxSource,
    A: ApprovalInboxSource,
    S: SkillProposalInboxSource,
{
    #[tracing::instrument(skip(self), err)]
    async fn list_mine(&self, user_id: &str) -> Result<InboxView> {
        let escalations = self.escalations.list_for_user(user_id).await?;
        let approvals = self.approvals.list_for_user(user_id).await?;
        let proposals = self.skills.list_for_user(user_id).await?;
        Ok(merge_inbox(escalations, approvals, proposals))
    }
}

fn merge_inbox(
    escalations: UserEscalations,
    approvals: UserApprovals,
    proposals: UserProposals,
) -> InboxView {
    let mut items = Vec::new();
    items.extend(escalation_items(escalations));
    items.extend(approval_items(approvals));
    items.extend(proposal_items(proposals));
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    InboxView { items }
}

fn escalation_items(view: UserEscalations) -> Vec<InboxItem> {
    let mut items = Vec::with_capacity(view.assigned.len() + view.claimable.len());
    for escalation in view.assigned {
        items.push(escalation_item(escalation, true, false));
    }
    for escalation in view.claimable {
        items.push(escalation_item(escalation, false, true));
    }
    items
}

fn escalation_item(escalation: Escalation, assigned_to_me: bool, team_queued: bool) -> InboxItem {
    InboxItem {
        kind: InboxKind::Escalation,
        id: escalation.id,
        title: escalation.title,
        status: escalation.status.as_str().to_string(),
        created_at: escalation.created_at,
        href: format!("/escalations/{}", escalation.id),
        assigned_to_me,
        team_queued,
    }
}

fn approval_items(view: UserApprovals) -> Vec<InboxItem> {
    let mut items = Vec::with_capacity(view.assigned.len() + view.team_queue.len());
    for request in view.assigned {
        items.push(approval_item(request, true, false));
    }
    for request in view.team_queue {
        items.push(approval_item(request, false, true));
    }
    items
}

fn approval_item(request: ApprovalRequest, assigned_to_me: bool, team_queued: bool) -> InboxItem {
    InboxItem {
        kind: InboxKind::Approval,
        id: request.id,
        title: request.summary,
        status: request.status.as_str().to_string(),
        created_at: request.created_at,
        href: format!("/approvals/{}", request.id),
        assigned_to_me,
        team_queued,
    }
}

fn proposal_items(view: UserProposals) -> Vec<InboxItem> {
    let mut items = Vec::with_capacity(view.assigned.len() + view.team_queue.len());
    for proposal in view.assigned {
        items.push(proposal_item(proposal, true, false));
    }
    for proposal in view.team_queue {
        items.push(proposal_item(proposal, false, true));
    }
    items
}

fn proposal_item(proposal: SkillProposal, assigned_to_me: bool, team_queued: bool) -> InboxItem {
    InboxItem {
        kind: InboxKind::SkillProposal,
        id: proposal.id,
        title: proposal.proposed_name,
        status: proposal.status.as_str().to_string(),
        created_at: proposal.created_at,
        href: format!("/skill-proposals/{}", proposal.id),
        assigned_to_me,
        team_queued,
    }
}
