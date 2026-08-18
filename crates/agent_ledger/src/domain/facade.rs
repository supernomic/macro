//! Agent-facing ledger facade: capability and tenancy policy for ledger
//! access by agent principals.
//!
//! Inbound adapters verify the agent bearer token (authentication) and pass
//! the resulting [`VerifiedAgent`] here; this facade owns the authorization
//! policy: scope checks, session ownership, and org tenancy.

#[cfg(test)]
mod test;

use agent_identity::domain::model::VerifiedAgent;
use chrono::Utc;
use macro_uuid::Uuid;

use super::model::{
    AgentEvent, ExternalThreadKind, LedgerError, NewAgentEvent, Result, SessionMapping,
    SessionOutcome,
};
use super::ports::{LedgerService, SessionMappingRepo};

/// Scope required to create sessions and append events.
pub const SCOPE_LEDGER_APPEND: &str = "ledger:append";
/// Scope required to read a session's own history.
pub const SCOPE_LEDGER_QUERY: &str = "ledger:query";

/// Request to open (or resume) a session for a runtime conversation.
#[derive(Debug, Clone)]
pub struct OpenSession {
    /// The runtime (Flue) conversation id.
    pub runtime_conversation_id: String,
    /// External thread anchoring the session, when any.
    pub external_thread_kind: Option<ExternalThreadKind>,
    /// External thread key, when any.
    pub external_thread_key: Option<String>,
}

fn require_scope(agent: &VerifiedAgent, scope: &str) -> Result<()> {
    agent
        .require_scope(scope)
        .map_err(|_| LedgerError::MissingScope {
            required: scope.to_string(),
        })
}

/// Agent-facing ledger facade over the ledger service and session mappings.
#[derive(Debug, Clone)]
pub struct AgentLedgerFacade<L, M> {
    ledger: L,
    mappings: M,
}

impl<L: LedgerService, M: SessionMappingRepo> AgentLedgerFacade<L, M> {
    /// Build a facade over the given ledger service and mapping repo.
    pub fn new(ledger: L, mappings: M) -> Self {
        Self { ledger, mappings }
    }

    /// Resolve a session the agent is allowed to touch: the mapping must
    /// exist and belong to the same org as the agent principal.
    async fn owned_mapping(
        &self,
        agent: &VerifiedAgent,
        session_id: Uuid,
    ) -> Result<SessionMapping> {
        let mapping = self
            .mappings
            .find_by_session(session_id)
            .await?
            .ok_or(LedgerError::SessionNotFound)?;
        if mapping.org_id != agent.principal.org_id {
            // Tenancy: a principal never sees sessions outside its org, and
            // the error is indistinguishable from absence.
            return Err(LedgerError::SessionNotFound);
        }
        Ok(mapping)
    }

    /// Open a session for a runtime conversation, or return the existing one
    /// (idempotent by runtime conversation id).
    #[tracing::instrument(skip(self, agent, request), fields(agent = %agent.principal.slug), err)]
    pub async fn open_session(
        &self,
        agent: &VerifiedAgent,
        request: OpenSession,
    ) -> Result<SessionMapping> {
        require_scope(agent, SCOPE_LEDGER_APPEND)?;

        if request.external_thread_kind.is_some() != request.external_thread_key.is_some() {
            return Err(LedgerError::InvalidRequest(
                "external thread kind and key must be provided together".to_string(),
            ));
        }

        if let Some(existing) = self
            .mappings
            .find_by_runtime_conversation(&request.runtime_conversation_id)
            .await?
        {
            if existing.org_id != agent.principal.org_id {
                return Err(LedgerError::SessionNotFound);
            }
            return Ok(existing);
        }

        let mapping = SessionMapping {
            session_id: macro_uuid::generate_uuid_v7(),
            runtime_conversation_id: request.runtime_conversation_id,
            external_thread_kind: request.external_thread_kind,
            external_thread_key: request.external_thread_key,
            org_id: agent.principal.org_id,
            agent_principal_id: agent.principal.id,
            created_at: Utc::now(),
        };
        self.mappings.create_mapping(&mapping).await?;
        Ok(mapping)
    }

    /// Find the session anchored to an external thread, when the agent's org
    /// owns one.
    #[tracing::instrument(skip(self, agent), fields(agent = %agent.principal.slug), err)]
    pub async fn find_session_by_thread(
        &self,
        agent: &VerifiedAgent,
        kind: ExternalThreadKind,
        key: &str,
    ) -> Result<Option<SessionMapping>> {
        require_scope(agent, SCOPE_LEDGER_QUERY)?;
        let mapping = self.mappings.find_by_external_thread(&kind, key).await?;
        Ok(mapping.filter(|m| m.org_id == agent.principal.org_id))
    }

    /// Append events to a session the agent's org owns.
    #[tracing::instrument(
        skip(self, agent, events),
        fields(agent = %agent.principal.slug, n = events.len()),
        err
    )]
    pub async fn append_events(
        &self,
        agent: &VerifiedAgent,
        session_id: Uuid,
        events: Vec<NewAgentEvent>,
    ) -> Result<Vec<AgentEvent>> {
        require_scope(agent, SCOPE_LEDGER_APPEND)?;
        let mapping = self.owned_mapping(agent, session_id).await?;
        self.ledger
            .append_events(session_id, mapping.org_id, events)
            .await
    }

    /// Replay a session's events for the agent (resume, self-query).
    #[tracing::instrument(skip(self, agent), fields(agent = %agent.principal.slug), err)]
    pub async fn list_session_events(
        &self,
        agent: &VerifiedAgent,
        session_id: Uuid,
        from_seq: i64,
        limit: i64,
    ) -> Result<Vec<AgentEvent>> {
        require_scope(agent, SCOPE_LEDGER_QUERY)?;
        self.owned_mapping(agent, session_id).await?;
        self.ledger
            .list_session_events(session_id, from_seq, limit)
            .await
    }

    /// Query events across the agent's org (self-query over past attempts,
    /// approvals, escalations). The org filter is forced to the agent's org.
    #[tracing::instrument(skip(self, agent, filter), fields(agent = %agent.principal.slug), err)]
    pub async fn query_org_events(
        &self,
        agent: &VerifiedAgent,
        mut filter: super::ports::EventFilter,
    ) -> Result<Vec<AgentEvent>> {
        require_scope(agent, SCOPE_LEDGER_QUERY)?;
        filter.org_id = agent.principal.org_id;
        if agent.principal.org_id.is_none() {
            // Platform-level principals must pin a session to query.
            if filter.session_id.is_none() {
                return Err(LedgerError::InvalidRequest(
                    "platform-level agents must query by session".to_string(),
                ));
            }
        }
        self.ledger.query_events(filter).await
    }

    /// Record a session's terminal outcome.
    #[tracing::instrument(skip(self, agent, summary), fields(agent = %agent.principal.slug), err)]
    pub async fn record_outcome(
        &self,
        agent: &VerifiedAgent,
        session_id: Uuid,
        outcome: SessionOutcome,
        summary: Option<String>,
    ) -> Result<()> {
        require_scope(agent, SCOPE_LEDGER_APPEND)?;
        self.owned_mapping(agent, session_id).await?;
        self.ledger
            .record_outcome(
                session_id,
                outcome,
                summary,
                super::model::Actor {
                    kind: super::model::ActorKind::Agent,
                    id: agent.principal.id.to_string(),
                },
            )
            .await
    }
}
