use super::*;
use crate::domain::model::RatingValue;
use crate::domain::ports::{ConsentRepo, RatingRepo};
use crate::domain::service::FeedbackServiceImpl;
use agent_identity::domain::model::{AgentKind, AgentPrincipal, VerifiedAgent};
use chrono::Utc;
use macro_uuid::Uuid;
use std::sync::Mutex;

struct MemRatings(Mutex<Vec<MessageRating>>);
struct MemConsent(Mutex<Vec<ConsentRecord>>);

impl RatingRepo for MemRatings {
    async fn upsert(&self, rating: &MessageRating) -> Result<MessageRating> {
        let mut rows = self.0.lock().unwrap();
        rows.retain(|r| {
            !(r.session_id == rating.session_id
                && r.target_seq == rating.target_seq
                && r.rated_by == rating.rated_by)
        });
        rows.push(rating.clone());
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
        let mut rows = self.0.lock().unwrap();
        rows.retain(|c| c.session_id != consent.session_id);
        rows.push(consent.clone());
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

fn agent(scopes: &[&str]) -> VerifiedAgent {
    VerifiedAgent {
        principal: AgentPrincipal {
            id: macro_uuid::generate_uuid_v7(),
            org_id: Some(1),
            slug: "super-agent".into(),
            display_name: "Super".into(),
            kind: AgentKind::SuperAgent,
            created_at: Utc::now(),
            disabled_at: None,
        },
        token_id: macro_uuid::generate_uuid_v7(),
        scopes: scopes.iter().map(|s| (*s).to_string()).collect(),
    }
}

#[tokio::test]
async fn rate_requires_scope() {
    let facade = AgentFeedbackFacade::new(FeedbackServiceImpl::new(
        MemRatings(Mutex::new(vec![])),
        MemConsent(Mutex::new(vec![])),
    ));
    let err = facade
        .rate(
            &agent(&[]),
            macro_uuid::generate_uuid_v7(),
            1,
            RatingValue::Up,
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, FeedbackError::MissingScope { .. }));
}

#[tokio::test]
async fn rate_and_list() {
    let facade = AgentFeedbackFacade::new(FeedbackServiceImpl::new(
        MemRatings(Mutex::new(vec![])),
        MemConsent(Mutex::new(vec![])),
    ));
    let session = macro_uuid::generate_uuid_v7();
    let rec = facade
        .rate(
            &agent(&["feedback:write", "feedback:read"]),
            session,
            3,
            RatingValue::Down,
            Some("too terse".into()),
        )
        .await
        .unwrap();
    assert_eq!(rec.target_seq, 3);
    let list = facade
        .list_ratings(&agent(&["feedback:read"]), session)
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
}

#[tokio::test]
async fn rate_as_user_rejects_empty_rated_by() {
    let facade = AgentFeedbackFacade::new(FeedbackServiceImpl::new(
        MemRatings(Mutex::new(vec![])),
        MemConsent(Mutex::new(vec![])),
    ));
    let err = facade
        .rate_as_user(
            macro_uuid::generate_uuid_v7(),
            1,
            RatingValue::None,
            None,
            " ".into(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, FeedbackError::InvalidRequest(_)));
}
