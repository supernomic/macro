//! Postgres implementation of the agent identity storage port.

use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{AgentApiToken, AgentKind, AgentPrincipal, IdentityError, Result};
use crate::domain::ports::AgentIdentityRepo;

/// Postgres-backed identity repo over the `agent_principals` and
/// `agent_api_tokens` tables.
#[derive(Debug, Clone)]
pub struct PgAgentIdentityRepo {
    pool: PgPool,
}

impl PgAgentIdentityRepo {
    /// Build a repo over the given pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct PrincipalRow {
    id: Uuid,
    org_id: Option<i32>,
    slug: String,
    display_name: String,
    kind: String,
    created_at: chrono::DateTime<chrono::Utc>,
    disabled_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn row_to_principal(row: PrincipalRow) -> Result<AgentPrincipal> {
    let kind = AgentKind::parse(&row.kind).ok_or_else(|| {
        IdentityError::InvalidRequest(format!("unknown agent kind: {}", row.kind))
    })?;
    Ok(AgentPrincipal {
        id: row.id,
        org_id: row.org_id,
        slug: row.slug,
        display_name: row.display_name,
        kind,
        created_at: row.created_at,
        disabled_at: row.disabled_at,
    })
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation())
}

impl AgentIdentityRepo for PgAgentIdentityRepo {
    async fn insert_principal(&self, principal: &AgentPrincipal) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO agent_principals (id, org_id, slug, display_name, kind, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            principal.id,
            principal.org_id,
            principal.slug,
            principal.display_name,
            principal.kind.as_str(),
            principal.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                IdentityError::SlugTaken
            } else {
                e.into()
            }
        })?;
        Ok(())
    }

    async fn get_principal(&self, id: Uuid) -> Result<Option<AgentPrincipal>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, slug, display_name, kind, created_at, disabled_at
            FROM agent_principals
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| {
            row_to_principal(PrincipalRow {
                id: r.id,
                org_id: r.org_id,
                slug: r.slug,
                display_name: r.display_name,
                kind: r.kind,
                created_at: r.created_at,
                disabled_at: r.disabled_at,
            })
        })
        .transpose()
    }

    async fn get_principal_by_slug(
        &self,
        org_id: Option<i32>,
        slug: &str,
    ) -> Result<Option<AgentPrincipal>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, slug, display_name, kind, created_at, disabled_at
            FROM agent_principals
            WHERE org_id IS NOT DISTINCT FROM $1 AND slug = $2
            "#,
            org_id,
            slug,
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| {
            row_to_principal(PrincipalRow {
                id: r.id,
                org_id: r.org_id,
                slug: r.slug,
                display_name: r.display_name,
                kind: r.kind,
                created_at: r.created_at,
                disabled_at: r.disabled_at,
            })
        })
        .transpose()
    }

    async fn list_principals(&self, org_id: Option<i32>) -> Result<Vec<AgentPrincipal>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, org_id, slug, display_name, kind, created_at, disabled_at
            FROM agent_principals
            WHERE org_id IS NOT DISTINCT FROM $1
            ORDER BY created_at ASC
            "#,
            org_id,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| {
                row_to_principal(PrincipalRow {
                    id: r.id,
                    org_id: r.org_id,
                    slug: r.slug,
                    display_name: r.display_name,
                    kind: r.kind,
                    created_at: r.created_at,
                    disabled_at: r.disabled_at,
                })
            })
            .collect()
    }

    async fn disable_principal(&self, id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE agent_principals SET disabled_at = NOW()
            WHERE id = $1 AND disabled_at IS NULL
            "#,
            id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn insert_token(&self, token: &AgentApiToken) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO agent_api_tokens (
                id, principal_id, name, secret_sha256, scopes,
                created_at, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            token.id,
            token.principal_id,
            token.name,
            token.secret_sha256,
            &token.scopes,
            token.created_at,
            token.expires_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_token(&self, id: Uuid) -> Result<Option<AgentApiToken>> {
        let row = sqlx::query!(
            r#"
            SELECT id, principal_id, name, secret_sha256, scopes,
                   created_at, expires_at, revoked_at
            FROM agent_api_tokens
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| AgentApiToken {
            id: r.id,
            principal_id: r.principal_id,
            name: r.name,
            secret_sha256: r.secret_sha256,
            scopes: r.scopes,
            created_at: r.created_at,
            expires_at: r.expires_at,
            revoked_at: r.revoked_at,
        }))
    }

    async fn revoke_token(&self, id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE agent_api_tokens SET revoked_at = NOW()
            WHERE id = $1 AND revoked_at IS NULL
            "#,
            id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn touch_token(&self, id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE agent_api_tokens SET last_used_at = NOW()
            WHERE id = $1
            "#,
            id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
