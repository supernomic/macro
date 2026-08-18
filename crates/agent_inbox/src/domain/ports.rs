//! Ports for the three personal-inbox list sources.

use super::model::Result;
use approvals::domain::service::{ApprovalService, UserApprovals};
use escalations::domain::service::{EscalationService, UserEscalations};
use skill_governance::domain::service::{SkillGovernanceService, UserProposals};

/// Escalation personal inbox (`assigned` + unclaimed team queue).
pub trait EscalationInboxSource: Send + Sync + 'static {
    /// The caller's assigned and claimable escalations.
    fn list_for_user(&self, user_id: &str) -> impl Future<Output = Result<UserEscalations>> + Send;
}

/// Approval personal inbox (`assigned` + unassigned team queue).
pub trait ApprovalInboxSource: Send + Sync + 'static {
    /// The caller's assigned and team-queue approvals.
    fn list_for_user(&self, user_id: &str) -> impl Future<Output = Result<UserApprovals>> + Send;
}

/// Skill-proposal personal inbox (`assigned` + unassigned team queue).
pub trait SkillProposalInboxSource: Send + Sync + 'static {
    /// The caller's assigned and team-queue skill proposals.
    fn list_for_user(&self, user_id: &str) -> impl Future<Output = Result<UserProposals>> + Send;
}

impl<T: EscalationService> EscalationInboxSource for T {
    async fn list_for_user(&self, user_id: &str) -> Result<UserEscalations> {
        EscalationService::list_for_user(self, user_id)
            .await
            .map_err(Into::into)
    }
}

impl<T: ApprovalService> ApprovalInboxSource for T {
    async fn list_for_user(&self, user_id: &str) -> Result<UserApprovals> {
        ApprovalService::list_for_user(self, user_id)
            .await
            .map_err(Into::into)
    }
}

impl<T: SkillGovernanceService> SkillProposalInboxSource for T {
    async fn list_for_user(&self, user_id: &str) -> Result<UserProposals> {
        SkillGovernanceService::list_for_user(self, user_id)
            .await
            .map_err(Into::into)
    }
}
