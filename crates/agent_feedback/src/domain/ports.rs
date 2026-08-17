//! Ports for the feedback sidecar.

use super::model::{ConsentRecord, MessageRating, Result};
use macro_uuid::Uuid;

/// Editable ratings.
pub trait RatingRepo: Send + Sync + 'static {
    /// Insert or replace a rating for `(session, seq, rater)`.
    fn upsert(&self, rating: &MessageRating) -> impl Future<Output = Result<MessageRating>> + Send;

    /// List ratings on a session.
    fn list_for_session(
        &self,
        session_id: Uuid,
    ) -> impl Future<Output = Result<Vec<MessageRating>>> + Send;
}

/// Per-session consent.
pub trait ConsentRepo: Send + Sync + 'static {
    /// Insert or replace consent for a session.
    fn upsert(&self, consent: &ConsentRecord)
    -> impl Future<Output = Result<ConsentRecord>> + Send;

    /// Fetch consent.
    fn get(&self, session_id: Uuid) -> impl Future<Output = Result<Option<ConsentRecord>>> + Send;
}
