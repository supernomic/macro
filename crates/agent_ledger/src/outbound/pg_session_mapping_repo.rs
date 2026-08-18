//! Postgres implementation of the session mapping port.

use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{ExternalThreadKind, LedgerError, Result, SessionMapping};
use crate::domain::ports::SessionMappingRepo;

/// Postgres-backed session mapping repo over the `agent_session_map` table.
#[derive(Debug, Clone)]
pub struct PgSessionMappingRepo {
    pool: PgPool,
}

impl PgSessionMappingRepo {
    /// Build a repo over the given pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct MappingRow {
    session_id: Uuid,
    runtime_conversation_id: String,
    external_thread_kind: Option<String>,
    external_thread_key: Option<String>,
    org_id: Option<i32>,
    agent_principal_id: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
}

fn row_to_mapping(row: MappingRow) -> Result<SessionMapping> {
    let external_thread_kind = row
        .external_thread_kind
        .as_deref()
        .map(|s| {
            ExternalThreadKind::parse(s).ok_or_else(|| {
                LedgerError::InvalidRequest(format!("unknown external thread kind: {s}"))
            })
        })
        .transpose()?;
    Ok(SessionMapping {
        session_id: row.session_id,
        runtime_conversation_id: row.runtime_conversation_id,
        external_thread_kind,
        external_thread_key: row.external_thread_key,
        org_id: row.org_id,
        agent_principal_id: row.agent_principal_id,
        created_at: row.created_at,
    })
}

impl SessionMappingRepo for PgSessionMappingRepo {
    async fn create_mapping(&self, mapping: &SessionMapping) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO agent_session_map (
                session_id, runtime_conversation_id,
                external_thread_kind, external_thread_key,
                org_id, agent_principal_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            mapping.session_id,
            mapping.runtime_conversation_id,
            mapping.external_thread_kind.as_ref().map(|k| k.as_str()),
            mapping.external_thread_key.as_deref(),
            mapping.org_id,
            mapping.agent_principal_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_runtime_conversation(
        &self,
        runtime_conversation_id: &str,
    ) -> Result<Option<SessionMapping>> {
        let row = sqlx::query!(
            r#"
            SELECT session_id, runtime_conversation_id, external_thread_kind,
                   external_thread_key, org_id, agent_principal_id, created_at
            FROM agent_session_map
            WHERE runtime_conversation_id = $1
            "#,
            runtime_conversation_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| {
            row_to_mapping(MappingRow {
                session_id: r.session_id,
                runtime_conversation_id: r.runtime_conversation_id,
                external_thread_kind: r.external_thread_kind,
                external_thread_key: r.external_thread_key,
                org_id: r.org_id,
                agent_principal_id: r.agent_principal_id,
                created_at: r.created_at,
            })
        })
        .transpose()
    }

    async fn find_by_external_thread(
        &self,
        kind: &ExternalThreadKind,
        key: &str,
    ) -> Result<Option<SessionMapping>> {
        let row = sqlx::query!(
            r#"
            SELECT session_id, runtime_conversation_id, external_thread_kind,
                   external_thread_key, org_id, agent_principal_id, created_at
            FROM agent_session_map
            WHERE external_thread_kind = $1 AND external_thread_key = $2
            "#,
            kind.as_str(),
            key,
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| {
            row_to_mapping(MappingRow {
                session_id: r.session_id,
                runtime_conversation_id: r.runtime_conversation_id,
                external_thread_kind: r.external_thread_kind,
                external_thread_key: r.external_thread_key,
                org_id: r.org_id,
                agent_principal_id: r.agent_principal_id,
                created_at: r.created_at,
            })
        })
        .transpose()
    }

    async fn find_by_session(&self, session_id: Uuid) -> Result<Option<SessionMapping>> {
        let row = sqlx::query!(
            r#"
            SELECT session_id, runtime_conversation_id, external_thread_kind,
                   external_thread_key, org_id, agent_principal_id, created_at
            FROM agent_session_map
            WHERE session_id = $1
            "#,
            session_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| {
            row_to_mapping(MappingRow {
                session_id: r.session_id,
                runtime_conversation_id: r.runtime_conversation_id,
                external_thread_kind: r.external_thread_kind,
                external_thread_key: r.external_thread_key,
                org_id: r.org_id,
                agent_principal_id: r.agent_principal_id,
                created_at: r.created_at,
            })
        })
        .transpose()
    }
}
