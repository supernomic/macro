//! Agent-facing approval facade: scope and tenancy policy for agent
//! principals gating tool calls and polling decisions.

#[cfg(test)]
mod test;

use agent_identity::domain::model::VerifiedAgent;
use macro_uuid::Uuid;

use super::model::{
    ApprovalError, ApprovalRequest, ApprovalStatus, GateOutcome, GateRequest, Result,
};
use super::service::{ApprovalService, Caller};

/// Scope required to gate tool calls (and cancel own pending requests).
pub const SCOPE_APPROVAL_GATE: &str = "approval:gate";
/// Scope required to read approval-request status.
pub const SCOPE_APPROVAL_QUERY: &str = "approval:query";

fn require_scope(agent: &VerifiedAgent, scope: &str) -> Result<()> {
    agent
        .require_scope(scope)
        .map_err(|_| ApprovalError::MissingScope {
            required: scope.to_string(),
        })
}

/// Agent-facing facade over the approval service.
#[derive(Debug, Clone)]
pub struct AgentApprovalFacade<S> {
    service: S,
}

impl<S: ApprovalService> AgentApprovalFacade<S> {
    /// Build the facade over the approval service.
    pub fn new(service: S) -> Self {
        Self { service }
    }

    /// Gate a proposed tool call as an agent. Tenancy is forced to the
    /// agent's org and the agent slug to the principal's slug; the actor
    /// recorded on transitions is the principal.
    #[tracing::instrument(skip(self, agent, request), fields(agent = %agent.principal.slug, tool = %request.tool_name), err)]
    pub async fn gate(
        &self,
        agent: &VerifiedAgent,
        mut request: GateRequest,
    ) -> Result<GateOutcome> {
        require_scope(agent, SCOPE_APPROVAL_GATE)?;
        request.agent_slug = agent.principal.slug.clone();
        self.service
            .gate(
                agent.principal.org_id,
                request,
                &agent.principal.id.to_string(),
            )
            .await
    }

    /// Poll one approval request the agent's org owns.
    #[tracing::instrument(skip(self, agent), fields(agent = %agent.principal.slug), err)]
    pub async fn get(&self, agent: &VerifiedAgent, id: Uuid) -> Result<ApprovalRequest> {
        require_scope(agent, SCOPE_APPROVAL_QUERY)?;
        let request = self.service.get(&Caller::Internal, id).await?;
        if request.org_id != agent.principal.org_id {
            // Tenancy: indistinguishable from absence.
            return Err(ApprovalError::NotFound);
        }
        Ok(request)
    }

    /// Cancel one of the agent's own pending requests (e.g. the
    /// conversation moved on).
    #[tracing::instrument(skip(self, agent), fields(agent = %agent.principal.slug), err)]
    pub async fn cancel(&self, agent: &VerifiedAgent, id: Uuid) -> Result<ApprovalRequest> {
        require_scope(agent, SCOPE_APPROVAL_GATE)?;
        let request = self.service.get(&Caller::Internal, id).await?;
        if request.org_id != agent.principal.org_id || request.agent_slug != agent.principal.slug {
            return Err(ApprovalError::NotFound);
        }
        if request.status != ApprovalStatus::Pending {
            return Err(ApprovalError::InvalidStatus("not pending".to_string()));
        }
        self.service.cancel(&Caller::Internal, id).await
    }
}
