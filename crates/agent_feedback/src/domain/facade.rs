//! Agent-facing feedback facade: scope checks before sidecar writes.

#[cfg(test)]
mod test;

use agent_identity::domain::model::VerifiedAgent;
use macro_uuid::Uuid;

use super::model::{
    ConsentRecord, FeedbackError, FeedbackSharingMode, MessageRating, RatingValue, Result,
};
use super::service::FeedbackService;

/// Scope required to read ratings / consent.
pub const SCOPE_FEEDBACK_READ: &str = "feedback:read";
/// Scope required to write ratings.
pub const SCOPE_FEEDBACK_WRITE: &str = "feedback:write";

fn require_scope(agent: &VerifiedAgent, scope: &str) -> Result<()> {
    agent
        .require_scope(scope)
        .map_err(|_| FeedbackError::MissingScope {
            required: scope.to_string(),
        })
}

/// Agent-facing facade.
#[derive(Debug, Clone)]
pub struct AgentFeedbackFacade<S> {
    service: S,
}

impl<S: FeedbackService> AgentFeedbackFacade<S> {
    /// Build over the feedback service.
    pub fn new(service: S) -> Self {
        Self { service }
    }

    /// Upsert a sidecar rating as this agent.
    pub async fn rate(
        &self,
        agent: &VerifiedAgent,
        session_id: Uuid,
        target_seq: i64,
        rating: RatingValue,
        note: Option<String>,
    ) -> Result<MessageRating> {
        require_scope(agent, SCOPE_FEEDBACK_WRITE)?;
        self.service
            .rate(
                session_id,
                target_seq,
                rating,
                note,
                agent.principal.id.to_string(),
            )
            .await
    }

    /// List ratings on a session.
    pub async fn list_ratings(
        &self,
        agent: &VerifiedAgent,
        session_id: Uuid,
    ) -> Result<Vec<MessageRating>> {
        require_scope(agent, SCOPE_FEEDBACK_READ)?;
        self.service.list_ratings(session_id).await
    }

    /// Read consent.
    pub async fn get_consent(
        &self,
        agent: &VerifiedAgent,
        session_id: Uuid,
    ) -> Result<Option<ConsentRecord>> {
        require_scope(agent, SCOPE_FEEDBACK_READ)?;
        self.service.get_consent(session_id).await
    }

    /// Set consent as a human (no agent scope).
    pub async fn set_consent_as_user(
        &self,
        session_id: Uuid,
        org_id: Option<i32>,
        sharing_mode: FeedbackSharingMode,
        set_by: String,
    ) -> Result<ConsentRecord> {
        self.service
            .set_consent(session_id, org_id, sharing_mode, set_by)
            .await
    }

    /// Rate as a human (no agent scope).
    pub async fn rate_as_user(
        &self,
        session_id: Uuid,
        target_seq: i64,
        rating: RatingValue,
        note: Option<String>,
        rated_by: String,
    ) -> Result<MessageRating> {
        self.service
            .rate(session_id, target_seq, rating, note, rated_by)
            .await
    }
}
