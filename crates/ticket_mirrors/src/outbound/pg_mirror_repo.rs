//! Postgres ticket-mirror storage.

use chrono::Utc;
use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{MirrorError, MirrorProvider, Result, TicketMirror};
use crate::domain::ports::MirrorRepo;

/// Postgres-backed mirror repo.
#[derive(Debug, Clone)]
pub struct PgMirrorRepo {
    pool: PgPool,
}

impl PgMirrorRepo {
    /// Build over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct MirrorRow {
    id: Uuid,
    org_id: Option<i32>,
    provider: String,
    native_entity_type: String,
    native_entity_id: String,
    foreign_id: String,
    foreign_url: Option<String>,
    summary: String,
    status: String,
    last_mirrored_at: chrono::DateTime<Utc>,
    disconnected_at: Option<chrono::DateTime<Utc>>,
}

fn row_to_mirror(row: MirrorRow) -> Result<TicketMirror> {
    Ok(TicketMirror {
        id: row.id,
        org_id: row.org_id,
        provider: MirrorProvider::parse(&row.provider).ok_or_else(|| {
            MirrorError::InvalidRequest(format!("unknown provider: {}", row.provider))
        })?,
        native_entity_type: row.native_entity_type,
        native_entity_id: row.native_entity_id,
        foreign_id: row.foreign_id,
        foreign_url: row.foreign_url,
        summary: row.summary,
        status: row.status,
        last_mirrored_at: row.last_mirrored_at,
        disconnected_at: row.disconnected_at,
    })
}

macro_rules! mirror_from_record {
    ($r:expr) => {
        row_to_mirror(MirrorRow {
            id: $r.id,
            org_id: $r.org_id,
            provider: $r.provider,
            native_entity_type: $r.native_entity_type,
            native_entity_id: $r.native_entity_id,
            foreign_id: $r.foreign_id,
            foreign_url: $r.foreign_url,
            summary: $r.summary,
            status: $r.status,
            last_mirrored_at: $r.last_mirrored_at,
            disconnected_at: $r.disconnected_at,
        })
    };
}

impl MirrorRepo for PgMirrorRepo {
    #[tracing::instrument(skip(self, mirror), err)]
    async fn upsert(&self, mirror: &TicketMirror) -> Result<TicketMirror> {
        // ON CONFLICT does not rewrite `id` / native identity; RETURNING keeps
        // the live row. Domain `find_active` also reuses that id before insert.
        let row = sqlx::query!(
            r#"
            INSERT INTO ticket_mirrors (
                id, org_id, provider, native_entity_type, native_entity_id,
                foreign_id, foreign_url, summary, status, last_mirrored_at,
                disconnected_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (org_id, native_entity_type, native_entity_id, provider)
                WHERE disconnected_at IS NULL
            DO UPDATE SET
                foreign_id = EXCLUDED.foreign_id,
                foreign_url = EXCLUDED.foreign_url,
                summary = EXCLUDED.summary,
                status = EXCLUDED.status,
                last_mirrored_at = EXCLUDED.last_mirrored_at
            RETURNING id, org_id, provider, native_entity_type, native_entity_id,
                      foreign_id, foreign_url, summary, status, last_mirrored_at,
                      disconnected_at
            "#,
            mirror.id,
            mirror.org_id,
            mirror.provider.as_str(),
            mirror.native_entity_type,
            mirror.native_entity_id,
            mirror.foreign_id,
            mirror.foreign_url.as_deref(),
            mirror.summary,
            mirror.status,
            mirror.last_mirrored_at,
            mirror.disconnected_at,
        )
        .fetch_one(&self.pool)
        .await?;
        mirror_from_record!(row)
    }

    #[tracing::instrument(skip(self), err)]
    async fn get(&self, id: Uuid) -> Result<Option<TicketMirror>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, provider, native_entity_type, native_entity_id,
                   foreign_id, foreign_url, summary, status, last_mirrored_at,
                   disconnected_at
            FROM ticket_mirrors
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| mirror_from_record!(r)).transpose()
    }

    #[tracing::instrument(skip(self), err)]
    async fn disconnect(&self, id: Uuid) -> Result<Option<TicketMirror>> {
        let row = sqlx::query!(
            r#"
            UPDATE ticket_mirrors
            SET disconnected_at = now()
            WHERE id = $1 AND disconnected_at IS NULL
            RETURNING id, org_id, provider, native_entity_type, native_entity_id,
                      foreign_id, foreign_url, summary, status, last_mirrored_at,
                      disconnected_at
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| mirror_from_record!(r)).transpose()
    }

    #[tracing::instrument(skip(self), err)]
    async fn find_active(
        &self,
        org_id: Option<i32>,
        native_entity_type: &str,
        native_entity_id: &str,
        provider: MirrorProvider,
    ) -> Result<Option<TicketMirror>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, provider, native_entity_type, native_entity_id,
                   foreign_id, foreign_url, summary, status, last_mirrored_at,
                   disconnected_at
            FROM ticket_mirrors
            WHERE org_id IS NOT DISTINCT FROM $1
              AND native_entity_type = $2
              AND native_entity_id = $3
              AND provider = $4
              AND disconnected_at IS NULL
            "#,
            org_id,
            native_entity_type,
            native_entity_id,
            provider.as_str(),
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| mirror_from_record!(r)).transpose()
    }
}
