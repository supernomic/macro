//! Consent reader over the feedback sidecar store.

use std::collections::HashMap;

use agent_feedback::domain::model::{FeedbackError, FeedbackSharingMode};
use agent_feedback::domain::ports::ConsentRepo;
use agent_feedback::outbound::PgConsentRepo;
use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{ExportError, Result, SharingMode};
use crate::domain::ports::ConsentReader;

/// Postgres-backed [`ConsentReader`] over `agent_session_consent`.
///
/// Wraps the feedback crate's store so this crate does not issue a second
/// SQL dialect against the same table.
#[derive(Debug, Clone)]
pub struct PgConsentReader {
    inner: PgConsentRepo,
}

impl PgConsentReader {
    /// Build over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self {
            inner: PgConsentRepo::new(pool),
        }
    }
}

fn sharing_mode_from_feedback(mode: FeedbackSharingMode) -> SharingMode {
    match mode {
        FeedbackSharingMode::Full => SharingMode::Full,
        FeedbackSharingMode::FeedbackOnly => SharingMode::FeedbackOnly,
        FeedbackSharingMode::Disabled => SharingMode::Disabled,
    }
}

fn map_feedback_error(error: FeedbackError) -> ExportError {
    match error {
        FeedbackError::InvalidRequest(msg) => ExportError::InvalidRequest(msg),
        FeedbackError::NotFound => ExportError::NotFound,
        FeedbackError::MissingScope { required } => {
            ExportError::InvalidRequest(format!("missing scope {required}"))
        }
        FeedbackError::Database(e) => ExportError::Database(e),
    }
}

impl ConsentReader for PgConsentReader {
    #[tracing::instrument(skip(self, session_ids), err)]
    async fn sharing_modes(&self, session_ids: &[Uuid]) -> Result<HashMap<Uuid, SharingMode>> {
        let mut out = HashMap::with_capacity(session_ids.len());
        for session_id in session_ids {
            if out.contains_key(session_id) {
                continue;
            }
            if let Some(record) = self
                .inner
                .get(*session_id)
                .await
                .map_err(map_feedback_error)?
            {
                out.insert(*session_id, sharing_mode_from_feedback(record.sharing_mode));
            }
        }
        Ok(out)
    }
}
