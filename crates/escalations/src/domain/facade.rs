//! Agent-facing escalation facade: scope and tenancy policy for agent
//! principals creating and polling escalations.

#[cfg(test)]
mod test;

use agent_identity::domain::model::VerifiedAgent;
use macro_uuid::Uuid;

use super::model::{Escalation, EscalationError, NewEscalation, Result};
use super::service::{Caller, EscalationService};

/// Scope required to create escalations.
pub const SCOPE_ESCALATION_CREATE: &str = "escalation:create";
/// Scope required to read escalation status.
pub const SCOPE_ESCALATION_QUERY: &str = "escalation:query";

fn require_scope(agent: &VerifiedAgent, scope: &str) -> Result<()> {
    agent
        .require_scope(scope)
        .map_err(|_| EscalationError::MissingScope {
            required: scope.to_string(),
        })
}

/// Agent-facing facade over the escalation service.
#[derive(Debug, Clone)]
pub struct AgentEscalationFacade<S> {
    service: S,
}

impl<S: EscalationService> AgentEscalationFacade<S> {
    /// Build the facade over the escalation service.
    pub fn new(service: S) -> Self {
        Self { service }
    }

    /// Create an escalation as an agent. Tenancy is forced to the agent's
    /// org; the actor recorded on the routing transition is the principal.
    #[tracing::instrument(skip(self, agent, request), fields(agent = %agent.principal.slug), err)]
    pub async fn create(
        &self,
        agent: &VerifiedAgent,
        request: NewEscalation,
    ) -> Result<Escalation> {
        require_scope(agent, SCOPE_ESCALATION_CREATE)?;
        self.service
            .create(
                agent.principal.org_id,
                request,
                &agent.principal.id.to_string(),
            )
            .await
    }

    /// Poll one escalation the agent's org owns.
    #[tracing::instrument(skip(self, agent), fields(agent = %agent.principal.slug), err)]
    pub async fn get(&self, agent: &VerifiedAgent, id: Uuid) -> Result<Escalation> {
        require_scope(agent, SCOPE_ESCALATION_QUERY)?;
        let escalation = self.service.get(&Caller::Internal, id).await?;
        if escalation.org_id != agent.principal.org_id {
            // Tenancy: indistinguishable from absence.
            return Err(EscalationError::NotFound);
        }
        Ok(escalation)
    }
}
