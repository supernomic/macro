use super::*;
use crate::domain::ports::{ConsentRepo, RatingRepo};
use chrono::Utc;
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

    async fn list_for_sessions(&self, session_ids: &[Uuid]) -> Result<Vec<ConsentRecord>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|c| session_ids.contains(&c.session_id))
            .cloned()
            .collect())
    }
}

fn svc() -> FeedbackServiceImpl<MemRatings, MemConsent> {
    FeedbackServiceImpl::new(
        MemRatings(Mutex::new(vec![])),
        MemConsent(Mutex::new(vec![])),
    )
}

#[tokio::test]
async fn rejects_negative_seq() {
    let err = svc()
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

#[tokio::test]
async fn rejects_empty_rated_by() {
    let err = svc()
        .rate(
            macro_uuid::generate_uuid_v7(),
            0,
            RatingValue::Down,
            None,
            "   ".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, FeedbackError::InvalidRequest(_)));
}

#[tokio::test]
async fn rejects_empty_set_by_on_consent() {
    let err = svc()
        .set_consent(
            macro_uuid::generate_uuid_v7(),
            Some(1),
            FeedbackSharingMode::FeedbackOnly,
            "".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, FeedbackError::InvalidRequest(_)));
}

#[tokio::test]
async fn upserts_sidecar_rating_without_touching_a_ledger() {
    let rec = svc()
        .rate(
            macro_uuid::generate_uuid_v7(),
            0,
            RatingValue::Up,
            Some("note".into()),
            "  user-1  ".into(),
        )
        .await
        .unwrap();
    assert_eq!(rec.target_seq, 0);
    assert_eq!(rec.rating, RatingValue::Up);
    assert_eq!(rec.rated_by, "user-1");
    assert_eq!(rec.note.as_deref(), Some("note"));
}

#[tokio::test]
async fn list_for_sessions_omits_missing_rows() {
    let consents = MemConsent(Mutex::new(vec![]));
    let present = macro_uuid::generate_uuid_v7();
    let missing = macro_uuid::generate_uuid_v7();
    consents
        .upsert(&ConsentRecord {
            session_id: present,
            org_id: Some(1),
            sharing_mode: FeedbackSharingMode::Full,
            set_by: "u".into(),
            updated_at: Utc::now(),
        })
        .await
        .unwrap();
    let listed = consents
        .list_for_sessions(&[present, missing])
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].session_id, present);
}
