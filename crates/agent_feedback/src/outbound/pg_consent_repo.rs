//! Postgres consent sidecar.

use chrono::Utc;
use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{ConsentRecord, FeedbackError, FeedbackSharingMode, Result};
use crate::domain::ports::ConsentRepo;

/// Postgres-backed consent repo.
#[derive(Debug, Clone)]
pub struct PgConsentRepo {
    pool: PgPool,
}

impl PgConsentRepo {
    /// Build over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn parse_row(
    session_id: Uuid,
    org_id: Option<i32>,
    sharing_mode: String,
    set_by: String,
    updated_at: chrono::DateTime<Utc>,
) -> Result<ConsentRecord> {
    Ok(ConsentRecord {
        session_id,
        org_id,
        sharing_mode: FeedbackSharingMode::parse(&sharing_mode).ok_or_else(|| {
            FeedbackError::InvalidRequest(format!("unknown sharing mode: {sharing_mode}"))
        })?,
        set_by,
        updated_at,
    })
}

impl ConsentRepo for PgConsentRepo {
    #[tracing::instrument(skip(self, consent), err)]
    async fn upsert(&self, consent: &ConsentRecord) -> Result<ConsentRecord> {
        let row = sqlx::query!(
            r#"
            INSERT INTO agent_session_consent (
                session_id, org_id, sharing_mode, set_by, updated_at
            )
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (session_id)
            DO UPDATE SET
                org_id = EXCLUDED.org_id,
                sharing_mode = EXCLUDED.sharing_mode,
                set_by = EXCLUDED.set_by,
                updated_at = EXCLUDED.updated_at
            RETURNING session_id, org_id, sharing_mode, set_by, updated_at
            "#,
            consent.session_id,
            consent.org_id,
            consent.sharing_mode.as_str(),
            consent.set_by,
            consent.updated_at,
        )
        .fetch_one(&self.pool)
        .await?;
        parse_row(
            row.session_id,
            row.org_id,
            row.sharing_mode,
            row.set_by,
            row.updated_at,
        )
    }

    #[tracing::instrument(skip(self), err)]
    async fn get(&self, session_id: Uuid) -> Result<Option<ConsentRecord>> {
        let row = sqlx::query!(
            r#"
            SELECT session_id, org_id, sharing_mode, set_by, updated_at
            FROM agent_session_consent
            WHERE session_id = $1
            "#,
            session_id
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            parse_row(
                r.session_id,
                r.org_id,
                r.sharing_mode,
                r.set_by,
                r.updated_at,
            )
        })
        .transpose()
    }

    #[tracing::instrument(skip(self, session_ids), err)]
    async fn list_for_sessions(&self, session_ids: &[Uuid]) -> Result<Vec<ConsentRecord>> {
        if session_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query!(
            r#"
            SELECT session_id, org_id, sharing_mode, set_by, updated_at
            FROM agent_session_consent
            WHERE session_id = ANY($1)
            "#,
            session_ids
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                parse_row(
                    r.session_id,
                    r.org_id,
                    r.sharing_mode,
                    r.set_by,
                    r.updated_at,
                )
            })
            .collect()
    }
}
