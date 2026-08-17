//! Postgres implementation of governed-skill storage.

use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{
    GovernanceError, OkfStatus, Result, SkillRecord, SkillScope, SkillSnapshot, TrustTier,
};
use crate::domain::ports::{SkillFilter, SkillRepo};

/// Postgres-backed skill repo.
#[derive(Debug, Clone)]
pub struct PgSkillRepo {
    pool: PgPool,
}

impl PgSkillRepo {
    /// Build over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn parse_skill(row: SkillRow) -> Result<SkillRecord> {
    Ok(SkillRecord {
        id: row.id,
        org_id: row.org_id,
        scope: SkillScope::parse(&row.scope).ok_or_else(|| {
            GovernanceError::InvalidRequest(format!("unknown scope: {}", row.scope))
        })?,
        owner_user_id: row.owner_user_id,
        owner_team_id: row.owner_team_id,
        slug: row.slug,
        name: row.name,
        description: row.description,
        body: row.body,
        trust_tier: TrustTier::parse(&row.trust_tier).ok_or_else(|| {
            GovernanceError::InvalidRequest(format!("unknown trust tier: {}", row.trust_tier))
        })?,
        okf_type: row.okf_type,
        okf_sources: serde_json::from_value(row.okf_sources).unwrap_or_default(),
        okf_generated: row.okf_generated,
        okf_verified: row.okf_verified,
        okf_status: OkfStatus::parse(&row.okf_status).ok_or_else(|| {
            GovernanceError::InvalidRequest(format!("unknown status: {}", row.okf_status))
        })?,
        stale_after: row.stale_after,
        content_hash: row.content_hash,
        version: row.version,
        created_at: row.created_at,
        updated_at: row.updated_at,
        archived_at: row.archived_at,
    })
}

struct SkillRow {
    id: Uuid,
    org_id: Option<i32>,
    scope: String,
    owner_user_id: Option<String>,
    owner_team_id: Option<Uuid>,
    slug: String,
    name: String,
    description: String,
    body: String,
    trust_tier: String,
    okf_type: String,
    okf_sources: serde_json::Value,
    okf_generated: bool,
    okf_verified: bool,
    okf_status: String,
    stale_after: Option<chrono::DateTime<chrono::Utc>>,
    content_hash: String,
    version: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SkillRepo for PgSkillRepo {
    #[tracing::instrument(skip(self, skill), err)]
    async fn insert(&self, skill: &SkillRecord) -> Result<()> {
        let sources = serde_json::to_value(&skill.okf_sources).unwrap_or(serde_json::json!([]));
        sqlx::query!(
            r#"
            INSERT INTO agent_skills (
                id, org_id, scope, owner_user_id, owner_team_id, slug, name,
                description, body, trust_tier, okf_type, okf_sources,
                okf_generated, okf_verified, okf_status, stale_after,
                content_hash, version, created_at, updated_at, archived_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                $8, $9, $10, $11, $12,
                $13, $14, $15, $16,
                $17, $18, $19, $20, $21
            )
            "#,
            skill.id,
            skill.org_id,
            skill.scope.as_str(),
            skill.owner_user_id.as_deref(),
            skill.owner_team_id,
            skill.slug,
            skill.name,
            skill.description,
            skill.body,
            skill.trust_tier.as_str(),
            skill.okf_type,
            sources,
            skill.okf_generated,
            skill.okf_verified,
            skill.okf_status.as_str(),
            skill.stale_after,
            skill.content_hash,
            skill.version,
            skill.created_at,
            skill.updated_at,
            skill.archived_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), err)]
    async fn get(&self, id: Uuid) -> Result<Option<SkillRecord>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, scope, owner_user_id, owner_team_id, slug, name,
                   description, body, trust_tier, okf_type, okf_sources,
                   okf_generated, okf_verified, okf_status, stale_after,
                   content_hash, version, created_at, updated_at, archived_at
            FROM agent_skills
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            parse_skill(SkillRow {
                id: r.id,
                org_id: r.org_id,
                scope: r.scope,
                owner_user_id: r.owner_user_id,
                owner_team_id: r.owner_team_id,
                slug: r.slug,
                name: r.name,
                description: r.description,
                body: r.body,
                trust_tier: r.trust_tier,
                okf_type: r.okf_type,
                okf_sources: r.okf_sources,
                okf_generated: r.okf_generated,
                okf_verified: r.okf_verified,
                okf_status: r.okf_status,
                stale_after: r.stale_after,
                content_hash: r.content_hash,
                version: r.version,
                created_at: r.created_at,
                updated_at: r.updated_at,
                archived_at: r.archived_at,
            })
        })
        .transpose()
    }

    #[tracing::instrument(skip(self), err)]
    async fn find_by_slug(
        &self,
        org_id: Option<i32>,
        scope: SkillScope,
        slug: &str,
        owner_user_id: Option<&str>,
        owner_team_id: Option<Uuid>,
    ) -> Result<Option<SkillRecord>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, scope, owner_user_id, owner_team_id, slug, name,
                   description, body, trust_tier, okf_type, okf_sources,
                   okf_generated, okf_verified, okf_status, stale_after,
                   content_hash, version, created_at, updated_at, archived_at
            FROM agent_skills
            WHERE org_id IS NOT DISTINCT FROM $1
              AND scope = $2
              AND slug = $3
              AND owner_user_id IS NOT DISTINCT FROM $4
              AND owner_team_id IS NOT DISTINCT FROM $5
              AND archived_at IS NULL
            "#,
            org_id,
            scope.as_str(),
            slug,
            owner_user_id,
            owner_team_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            parse_skill(SkillRow {
                id: r.id,
                org_id: r.org_id,
                scope: r.scope,
                owner_user_id: r.owner_user_id,
                owner_team_id: r.owner_team_id,
                slug: r.slug,
                name: r.name,
                description: r.description,
                body: r.body,
                trust_tier: r.trust_tier,
                okf_type: r.okf_type,
                okf_sources: r.okf_sources,
                okf_generated: r.okf_generated,
                okf_verified: r.okf_verified,
                okf_status: r.okf_status,
                stale_after: r.stale_after,
                content_hash: r.content_hash,
                version: r.version,
                created_at: r.created_at,
                updated_at: r.updated_at,
                archived_at: r.archived_at,
            })
        })
        .transpose()
    }

    #[tracing::instrument(skip(self), err)]
    async fn list(&self, filter: &SkillFilter) -> Result<Vec<SkillRecord>> {
        let limit = if filter.limit <= 0 { 100 } else { filter.limit };
        let rows = sqlx::query!(
            r#"
            SELECT id, org_id, scope, owner_user_id, owner_team_id, slug, name,
                   description, body, trust_tier, okf_type, okf_sources,
                   okf_generated, okf_verified, okf_status, stale_after,
                   content_hash, version, created_at, updated_at, archived_at
            FROM agent_skills
            WHERE archived_at IS NULL
              AND (
                scope = 'platform'
                OR (
                  org_id IS NOT DISTINCT FROM $1
                  AND (
                    scope = 'org'
                    OR (scope = 'user' AND $2::text IS NOT NULL AND owner_user_id = $2)
                    OR (
                      scope = 'team'
                      AND cardinality($3::uuid[]) > 0
                      AND owner_team_id = ANY($3::uuid[])
                    )
                  )
                )
              )
            ORDER BY updated_at DESC
            LIMIT $4
            "#,
            filter.org_id,
            filter.owner_user_id.as_deref(),
            &filter.owner_team_ids,
            limit,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                parse_skill(SkillRow {
                    id: r.id,
                    org_id: r.org_id,
                    scope: r.scope,
                    owner_user_id: r.owner_user_id,
                    owner_team_id: r.owner_team_id,
                    slug: r.slug,
                    name: r.name,
                    description: r.description,
                    body: r.body,
                    trust_tier: r.trust_tier,
                    okf_type: r.okf_type,
                    okf_sources: r.okf_sources,
                    okf_generated: r.okf_generated,
                    okf_verified: r.okf_verified,
                    okf_status: r.okf_status,
                    stale_after: r.stale_after,
                    content_hash: r.content_hash,
                    version: r.version,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                    archived_at: r.archived_at,
                })
            })
            .collect()
    }

    #[tracing::instrument(skip(self, skill), err)]
    async fn update(&self, skill: &SkillRecord) -> Result<()> {
        let sources = serde_json::to_value(&skill.okf_sources).unwrap_or(serde_json::json!([]));
        sqlx::query!(
            r#"
            UPDATE agent_skills SET
                name = $2,
                description = $3,
                body = $4,
                trust_tier = $5,
                okf_sources = $6,
                okf_generated = $7,
                okf_verified = $8,
                okf_status = $9,
                stale_after = $10,
                content_hash = $11,
                version = $12,
                updated_at = $13,
                archived_at = $14
            WHERE id = $1
            "#,
            skill.id,
            skill.name,
            skill.description,
            skill.body,
            skill.trust_tier.as_str(),
            sources,
            skill.okf_generated,
            skill.okf_verified,
            skill.okf_status.as_str(),
            skill.stale_after,
            skill.content_hash,
            skill.version,
            skill.updated_at,
            skill.archived_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self, snapshot), err)]
    async fn insert_snapshot(&self, snapshot: &SkillSnapshot) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO agent_skill_snapshots (
                id, skill_id, version, body, description, content_hash,
                created_at, created_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            snapshot.id,
            snapshot.skill_id,
            snapshot.version,
            snapshot.body,
            snapshot.description,
            snapshot.content_hash,
            snapshot.created_at,
            snapshot.created_by,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), err)]
    async fn get_snapshot(&self, id: Uuid) -> Result<Option<SkillSnapshot>> {
        let row = sqlx::query!(
            r#"
            SELECT id, skill_id, version, body, description, content_hash,
                   created_at, created_by
            FROM agent_skill_snapshots
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| SkillSnapshot {
            id: r.id,
            skill_id: r.skill_id,
            version: r.version,
            body: r.body,
            description: r.description,
            content_hash: r.content_hash,
            created_at: r.created_at,
            created_by: r.created_by,
        }))
    }
}
