//! Postgres connector storage.

use chrono::Utc;
use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{ConnectorAccount, ConnectorError, ConnectorRecord, Provider, Result};
use crate::domain::ports::ConnectorRepo;

/// Postgres-backed connector repo.
#[derive(Debug, Clone)]
pub struct PgConnectorRepo {
    pool: PgPool,
}

impl PgConnectorRepo {
    /// Build over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl ConnectorRepo for PgConnectorRepo {
    #[tracing::instrument(skip(self, account), err)]
    async fn insert_account(&self, account: &ConnectorAccount) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO lifecycle_connector_accounts (
                id, org_id, provider, display_name, credential_ref,
                last_synced_at, last_cursor, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            account.id,
            account.org_id,
            account.provider.as_str(),
            account.display_name,
            account.credential_ref,
            account.last_synced_at,
            account.last_cursor.as_deref(),
            account.created_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), err)]
    async fn get_account(&self, id: Uuid) -> Result<Option<ConnectorAccount>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, provider, display_name, credential_ref,
                   last_synced_at, last_cursor, created_at
            FROM lifecycle_connector_accounts
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            Ok(ConnectorAccount {
                id: r.id,
                org_id: r.org_id,
                provider: Provider::parse(&r.provider).ok_or_else(|| {
                    ConnectorError::InvalidRequest(format!("unknown provider: {}", r.provider))
                })?,
                display_name: r.display_name,
                credential_ref: r.credential_ref,
                last_synced_at: r.last_synced_at,
                last_cursor: r.last_cursor,
                created_at: r.created_at,
            })
        })
        .transpose()
    }

    #[tracing::instrument(skip(self), err)]
    async fn list_accounts(
        &self,
        org_id: Option<i32>,
        provider: Option<Provider>,
    ) -> Result<Vec<ConnectorAccount>> {
        let provider_s = provider.map(|p| p.as_str().to_string());
        let rows = sqlx::query!(
            r#"
            SELECT id, org_id, provider, display_name, credential_ref,
                   last_synced_at, last_cursor, created_at
            FROM lifecycle_connector_accounts
            WHERE org_id IS NOT DISTINCT FROM $1
              AND ($2::text IS NULL OR provider = $2)
            ORDER BY created_at
            "#,
            org_id,
            provider_s,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(ConnectorAccount {
                    id: r.id,
                    org_id: r.org_id,
                    provider: Provider::parse(&r.provider).ok_or_else(|| {
                        ConnectorError::InvalidRequest(format!("unknown provider: {}", r.provider))
                    })?,
                    display_name: r.display_name,
                    credential_ref: r.credential_ref,
                    last_synced_at: r.last_synced_at,
                    last_cursor: r.last_cursor,
                    created_at: r.created_at,
                })
            })
            .collect()
    }

    #[tracing::instrument(skip(self, record), err)]
    async fn upsert_record(&self, record: &ConnectorRecord) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO lifecycle_connector_records (
                id, account_id, external_id, record_type, payload, graph_node_id, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (account_id, record_type, external_id)
            DO UPDATE SET
                payload = EXCLUDED.payload,
                graph_node_id = EXCLUDED.graph_node_id,
                updated_at = EXCLUDED.updated_at
            "#,
            record.id,
            record.account_id,
            record.external_id,
            record.record_type,
            record.payload,
            record.graph_node_id,
            record.updated_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), err)]
    async fn touch_sync(&self, id: Uuid, cursor: Option<&str>) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE lifecycle_connector_accounts
            SET last_synced_at = $2, last_cursor = $3
            WHERE id = $1
            "#,
            id,
            Utc::now(),
            cursor,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
