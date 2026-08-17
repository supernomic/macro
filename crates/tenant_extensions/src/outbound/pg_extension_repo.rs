//! Postgres tenant-extension storage.

use chrono::Utc;
use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{
    ExtensionError, ExtensionSnapshot, ExtensionStatus, Result, TenantExtension,
};
use crate::domain::ports::ExtensionRepo;

/// Postgres-backed extension repo.
#[derive(Debug, Clone)]
pub struct PgExtensionRepo {
    pool: PgPool,
}

impl PgExtensionRepo {
    /// Build over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn parse_status(s: &str) -> Result<ExtensionStatus> {
    ExtensionStatus::parse(s)
        .ok_or_else(|| ExtensionError::InvalidRequest(format!("unknown status: {s}")))
}

impl ExtensionRepo for PgExtensionRepo {
    #[tracing::instrument(skip(self, ext), err)]
    async fn insert(&self, ext: &TenantExtension) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO tenant_extensions (
                id, org_id, slug, display_name, version, sdk_semver, manifest,
                artifact_hash, status, scopes, principal_id, activated_at,
                created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            "#,
            ext.id,
            ext.org_id,
            ext.slug,
            ext.display_name,
            ext.version,
            ext.sdk_semver,
            ext.manifest,
            ext.artifact_hash,
            ext.status.as_str(),
            &ext.scopes,
            ext.principal_id,
            ext.activated_at,
            ext.created_at,
            ext.updated_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), err)]
    async fn get(&self, id: Uuid) -> Result<Option<TenantExtension>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, slug, display_name, version, sdk_semver, manifest,
                   artifact_hash, status, scopes, principal_id, activated_at,
                   created_at, updated_at
            FROM tenant_extensions
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            Ok(TenantExtension {
                id: r.id,
                org_id: r.org_id,
                slug: r.slug,
                display_name: r.display_name,
                version: r.version,
                sdk_semver: r.sdk_semver,
                manifest: r.manifest,
                artifact_hash: r.artifact_hash,
                status: parse_status(&r.status)?,
                scopes: r.scopes,
                principal_id: r.principal_id,
                activated_at: r.activated_at,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
        })
        .transpose()
    }

    #[tracing::instrument(skip(self), err)]
    async fn find_by_slug(&self, org_id: i32, slug: &str) -> Result<Option<TenantExtension>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, slug, display_name, version, sdk_semver, manifest,
                   artifact_hash, status, scopes, principal_id, activated_at,
                   created_at, updated_at
            FROM tenant_extensions
            WHERE org_id = $1 AND slug = $2
            "#,
            org_id,
            slug,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            Ok(TenantExtension {
                id: r.id,
                org_id: r.org_id,
                slug: r.slug,
                display_name: r.display_name,
                version: r.version,
                sdk_semver: r.sdk_semver,
                manifest: r.manifest,
                artifact_hash: r.artifact_hash,
                status: parse_status(&r.status)?,
                scopes: r.scopes,
                principal_id: r.principal_id,
                activated_at: r.activated_at,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
        })
        .transpose()
    }

    #[tracing::instrument(skip(self, ext), err)]
    async fn update(&self, ext: &TenantExtension) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE tenant_extensions SET
                display_name = $2, version = $3, sdk_semver = $4, manifest = $5,
                artifact_hash = $6, status = $7, scopes = $8, principal_id = $9,
                activated_at = $10, updated_at = $11
            WHERE id = $1
            "#,
            ext.id,
            ext.display_name,
            ext.version,
            ext.sdk_semver,
            ext.manifest,
            ext.artifact_hash,
            ext.status.as_str(),
            &ext.scopes,
            ext.principal_id,
            ext.activated_at,
            ext.updated_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self, snapshot), err)]
    async fn insert_snapshot(&self, snapshot: &ExtensionSnapshot) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO tenant_extension_snapshots (
                id, extension_id, version, manifest, artifact_hash, status,
                created_at, created_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            snapshot.id,
            snapshot.extension_id,
            snapshot.version,
            snapshot.manifest,
            snapshot.artifact_hash,
            snapshot.status.as_str(),
            snapshot.created_at,
            snapshot.created_by,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), err)]
    async fn latest_snapshot(&self, extension_id: Uuid) -> Result<Option<ExtensionSnapshot>> {
        let row = sqlx::query!(
            r#"
            SELECT id, extension_id, version, manifest, artifact_hash, status,
                   created_at, created_by
            FROM tenant_extension_snapshots
            WHERE extension_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            extension_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            Ok(ExtensionSnapshot {
                id: r.id,
                extension_id: r.extension_id,
                version: r.version,
                manifest: r.manifest,
                artifact_hash: r.artifact_hash,
                status: parse_status(&r.status)?,
                created_at: r.created_at,
                created_by: r.created_by,
            })
        })
        .transpose()
    }

    #[tracing::instrument(skip(self), err)]
    #[allow(clippy::disallowed_methods)] // query_scalar! needs `.sqlx` from prepare_db
    async fn get_catalog(&self, org_id: i32) -> Result<Option<serde_json::Value>> {
        let catalog = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT catalog FROM tenant_extension_catalogs WHERE org_id = $1",
        )
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(catalog)
    }

    #[tracing::instrument(skip(self, catalog), err)]
    async fn upsert_catalog(
        &self,
        org_id: i32,
        catalog: serde_json::Value,
        updated_by: &str,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO tenant_extension_catalogs (org_id, catalog, updated_at, updated_by)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (org_id)
            DO UPDATE SET catalog = EXCLUDED.catalog, updated_at = EXCLUDED.updated_at,
                          updated_by = EXCLUDED.updated_by
            "#,
            org_id,
            catalog,
            Utc::now(),
            updated_by,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
