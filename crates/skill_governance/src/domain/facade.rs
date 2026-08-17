//! Agent-facing skills facade: scope and tenancy for catalog + proposals.

#[cfg(test)]
mod test;

use agent_identity::domain::model::VerifiedAgent;
use macro_uuid::Uuid;

use super::model::{
    GovernanceError, NewProposal, Result, SkillCatalogEntry, SkillProposal, SkillRecord,
};
use super::service::SkillGovernanceService;

/// Scope required to read the skill catalog.
pub const SCOPE_SKILL_READ: &str = "skill:read";
/// Scope required to open skill proposals.
pub const SCOPE_SKILL_PROPOSE: &str = "skill:propose";

fn require_scope(agent: &VerifiedAgent, scope: &str) -> Result<()> {
    agent
        .require_scope(scope)
        .map_err(|_| GovernanceError::MissingScope {
            required: scope.to_string(),
        })
}

/// Agent-facing facade over the governance service.
#[derive(Debug, Clone)]
pub struct AgentSkillFacade<S> {
    service: S,
}

impl<S: SkillGovernanceService> AgentSkillFacade<S> {
    /// Build the facade over the governance service.
    pub fn new(service: S) -> Self {
        Self { service }
    }

    /// Catalog of skills this agent may inject for its org.
    #[tracing::instrument(skip(self, agent), fields(agent = %agent.principal.slug), err)]
    pub async fn catalog(&self, agent: &VerifiedAgent) -> Result<Vec<SkillCatalogEntry>> {
        require_scope(agent, SCOPE_SKILL_READ)?;
        self.service
            .catalog_for(agent.principal.org_id, None, &[])
            .await
    }

    /// Fetch one skill in the agent's org.
    #[tracing::instrument(skip(self, agent), fields(agent = %agent.principal.slug), err)]
    pub async fn get_skill(&self, agent: &VerifiedAgent, id: Uuid) -> Result<SkillRecord> {
        require_scope(agent, SCOPE_SKILL_READ)?;
        let skill = self.service.get_skill(id).await?;
        if skill.org_id != agent.principal.org_id {
            return Err(GovernanceError::NotFound);
        }
        Ok(skill)
    }

    /// Open a staged proposal as this agent. Tenancy is forced to the
    /// principal's org.
    #[tracing::instrument(skip(self, agent, proposal), fields(agent = %agent.principal.slug), err)]
    pub async fn propose(
        &self,
        agent: &VerifiedAgent,
        proposal: NewProposal,
    ) -> Result<SkillProposal> {
        require_scope(agent, SCOPE_SKILL_PROPOSE)?;
        self.service
            .propose(
                agent.principal.org_id,
                proposal,
                Some(agent.principal.id.to_string()),
                None,
            )
            .await
    }
}
