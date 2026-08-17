//! Feedback domain service.

#[cfg(test)]
mod test;

use chrono::Utc;
use macro_uuid::Uuid;

use super::model::{
    ConsentRecord, FeedbackError, FeedbackSharingMode, MessageRating, RatingValue, Result,
};
use super::ports::{ConsentRepo, RatingRepo};

/// Domain service.
pub trait FeedbackService: Send + Sync + 'static {
    /// Upsert a sidecar rating.
    fn rate(
        &self,
        session_id: Uuid,
        target_seq: i64,
        rating: RatingValue,
        note: Option<String>,
        rated_by: String,
    ) -> impl Future<Output = Result<MessageRating>> + Send;

    /// List ratings for a session.
    fn list_ratings(
        &self,
        session_id: Uuid,
    ) -> impl Future<Output = Result<Vec<MessageRating>>> + Send;

    /// Set sharing consent.
    fn set_consent(
        &self,
        session_id: Uuid,
        org_id: Option<i32>,
        sharing_mode: FeedbackSharingMode,
        set_by: String,
    ) -> impl Future<Output = Result<ConsentRecord>> + Send;

    /// Fetch consent.
    fn get_consent(
        &self,
        session_id: Uuid,
    ) -> impl Future<Output = Result<Option<ConsentRecord>>> + Send;
}

/// Concrete service.
#[derive(Debug, Clone)]
pub struct FeedbackServiceImpl<R, C> {
    ratings: R,
    consent: C,
}

impl<R: RatingRepo, C: ConsentRepo> FeedbackServiceImpl<R, C> {
    /// Build over storage ports.
    pub fn new(ratings: R, consent: C) -> Self {
        Self { ratings, consent }
    }
}

impl<R: RatingRepo, C: ConsentRepo> FeedbackService for FeedbackServiceImpl<R, C> {
    #[tracing::instrument(skip(self, note), err)]
    async fn rate(
        &self,
        session_id: Uuid,
        target_seq: i64,
        rating: RatingValue,
        note: Option<String>,
        rated_by: String,
    ) -> Result<MessageRating> {
        if target_seq < 0 {
            return Err(FeedbackError::InvalidRequest(
                "target_seq must be >= 0".to_string(),
            ));
        }
        if rated_by.trim().is_empty() {
            return Err(FeedbackError::InvalidRequest(
                "rated_by is required".to_string(),
            ));
        }
        self.ratings
            .upsert(&MessageRating {
                session_id,
                target_seq,
                rating,
                note,
                rated_by,
                updated_at: Utc::now(),
            })
            .await
    }

    #[tracing::instrument(skip(self), err)]
    async fn list_ratings(&self, session_id: Uuid) -> Result<Vec<MessageRating>> {
        self.ratings.list_for_session(session_id).await
    }

    #[tracing::instrument(skip(self), err)]
    async fn set_consent(
        &self,
        session_id: Uuid,
        org_id: Option<i32>,
        sharing_mode: FeedbackSharingMode,
        set_by: String,
    ) -> Result<ConsentRecord> {
        if set_by.trim().is_empty() {
            return Err(FeedbackError::InvalidRequest(
                "set_by is required".to_string(),
            ));
        }
        self.consent
            .upsert(&ConsentRecord {
                session_id,
                org_id,
                sharing_mode,
                set_by,
                updated_at: Utc::now(),
            })
            .await
    }

    #[tracing::instrument(skip(self), err)]
    async fn get_consent(&self, session_id: Uuid) -> Result<Option<ConsentRecord>> {
        self.consent.get(session_id).await
    }
}
