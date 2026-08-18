//! Postgres rating sidecar.

use chrono::Utc;
use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{FeedbackError, MessageRating, RatingValue, Result};
use crate::domain::ports::RatingRepo;

/// Postgres-backed rating repo.
#[derive(Debug, Clone)]
pub struct PgRatingRepo {
    pool: PgPool,
}

impl PgRatingRepo {
    /// Build over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn parse_row(
    session_id: Uuid,
    target_seq: i64,
    rating: i16,
    note: Option<String>,
    rated_by: String,
    updated_at: chrono::DateTime<Utc>,
) -> Result<MessageRating> {
    Ok(MessageRating {
        session_id,
        target_seq,
        rating: RatingValue::from_i16(rating)
            .ok_or_else(|| FeedbackError::InvalidRequest(format!("unknown rating: {rating}")))?,
        note,
        rated_by,
        updated_at,
    })
}

impl RatingRepo for PgRatingRepo {
    #[tracing::instrument(skip(self, rating), err)]
    async fn upsert(&self, rating: &MessageRating) -> Result<MessageRating> {
        let row = sqlx::query!(
            r#"
            INSERT INTO agent_message_ratings (
                session_id, target_seq, rating, note, rated_by, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (session_id, target_seq, rated_by)
            DO UPDATE SET
                rating = EXCLUDED.rating,
                note = EXCLUDED.note,
                updated_at = EXCLUDED.updated_at
            RETURNING session_id, target_seq, rating, note, rated_by, updated_at
            "#,
            rating.session_id,
            rating.target_seq,
            rating.rating.as_i16(),
            rating.note,
            rating.rated_by,
            rating.updated_at,
        )
        .fetch_one(&self.pool)
        .await?;
        parse_row(
            row.session_id,
            row.target_seq,
            row.rating,
            row.note,
            row.rated_by,
            row.updated_at,
        )
    }

    #[tracing::instrument(skip(self), err)]
    async fn list_for_session(&self, session_id: Uuid) -> Result<Vec<MessageRating>> {
        let rows = sqlx::query!(
            r#"
            SELECT session_id, target_seq, rating, note, rated_by, updated_at
            FROM agent_message_ratings
            WHERE session_id = $1
            ORDER BY target_seq ASC
            "#,
            session_id
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                parse_row(
                    r.session_id,
                    r.target_seq,
                    r.rating,
                    r.note,
                    r.rated_by,
                    r.updated_at,
                )
            })
            .collect()
    }
}
