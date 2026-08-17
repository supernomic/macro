use super::*;
use crate::domain::ports::{ConsentRepo, RatingRepo};
use macro_uuid::Uuid;
use std::sync::Mutex;

struct MemRatings(Mutex<Vec<MessageRating>>);
struct MemConsent(Mutex<Vec<ConsentRecord>>);

impl RatingRepo for MemRatings {
    async fn upsert(&self, rating: &MessageRating) -> Result<MessageRating> {
        self.0.lock().unwrap().push(rating.clone());
        Ok(rating.clone())
    }
    async fn list_for_session(&self, session_id: Uuid) -> Result<Vec<MessageRating>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.session_id == session_id)
            .cloned()
            .collect())
    }
}

impl ConsentRepo for MemConsent {
    async fn upsert(&self, consent: &ConsentRecord) -> Result<ConsentRecord> {
        self.0.lock().unwrap().push(consent.clone());
        Ok(consent.clone())
    }
    async fn get(&self, session_id: Uuid) -> Result<Option<ConsentRecord>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|c| c.session_id == session_id)
            .cloned())
    }
}

#[tokio::test]
async fn rejects_negative_seq() {
    let svc = FeedbackServiceImpl::new(
        MemRatings(Mutex::new(vec![])),
        MemConsent(Mutex::new(vec![])),
    );
    let err = svc
        .rate(
            macro_uuid::generate_uuid_v7(),
            -1,
            RatingValue::Up,
            None,
            "u1".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, FeedbackError::InvalidRequest(_)));
}
