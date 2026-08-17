//! Tenant-extension domain service: register, activate, rollback, delist.

#[cfg(test)]
mod test;

use chrono::Utc;
use macro_uuid::Uuid;

use super::model::{
    ExtensionError, ExtensionSnapshot, ExtensionStatus, RegisterExtension, Result, TenantExtension,
};
use super::ports::ExtensionRepo;

/// Domain service.
pub trait ExtensionService: Send + Sync + 'static {
    /// Register a draft extension. Re-tagging an existing version with a
    /// different artifact hash is refused.
    fn register(
        &self,
        org_id: i32,
        request: RegisterExtension,
        actor: &str,
    ) -> impl Future<Output = Result<TenantExtension>> + Send;

    /// Candidate-set swap: snapshot the current live row, then mark this
    /// extension active. On a later failure, [`rollback`] restores the snapshot.
    fn activate(
        &self,
        id: Uuid,
        actor: &str,
    ) -> impl Future<Output = Result<TenantExtension>> + Send;

    /// Restore the latest snapshot (old set live).
    fn rollback(
        &self,
        id: Uuid,
        actor: &str,
    ) -> impl Future<Output = Result<TenantExtension>> + Send;

    /// Delist kill switch. Disabled extensions cannot be activated or rolled back.
    fn disable(
        &self,
        id: Uuid,
        actor: &str,
    ) -> impl Future<Output = Result<TenantExtension>> + Send;
}

/// Concrete service.
#[derive(Debug, Clone)]
pub struct ExtensionServiceImpl<R> {
    repo: R,
}

impl<R: ExtensionRepo> ExtensionServiceImpl<R> {
    /// Build over storage.
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
}

fn catalog_json(ext: &TenantExtension) -> serde_json::Value {
    serde_json::json!({
        "extensions": [{
            "slug": ext.slug,
            "version": ext.version,
            "artifact_hash": ext.artifact_hash,
            "status": ext.status.as_str(),
        }]
    })
}

fn snapshot_of(ext: &TenantExtension, actor: &str) -> ExtensionSnapshot {
    ExtensionSnapshot {
        id: macro_uuid::generate_uuid_v7(),
        extension_id: ext.id,
        version: ext.version.clone(),
        manifest: ext.manifest.clone(),
        artifact_hash: ext.artifact_hash.clone(),
        status: ext.status,
        created_at: Utc::now(),
        created_by: actor.to_string(),
    }
}

impl<R: ExtensionRepo> ExtensionService for ExtensionServiceImpl<R> {
    #[tracing::instrument(skip(self, request), err)]
    async fn register(
        &self,
        org_id: i32,
        request: RegisterExtension,
        actor: &str,
    ) -> Result<TenantExtension> {
        if request.slug.trim().is_empty() || request.artifact_hash.trim().is_empty() {
            return Err(ExtensionError::InvalidRequest(
                "slug and artifact_hash are required".to_string(),
            ));
        }
        if let Some(existing) = self.repo.find_by_slug(org_id, &request.slug).await? {
            if matches!(existing.status, ExtensionStatus::Disabled) {
                return Err(ExtensionError::InvalidStatus(
                    "disabled extensions cannot be updated".to_string(),
                ));
            }
            if existing.version == request.version
                && existing.artifact_hash != request.artifact_hash
            {
                return Err(ExtensionError::RetaggedRelease {
                    version: existing.version,
                    existing: existing.artifact_hash,
                });
            }
            let replacing_live = existing.version != request.version
                && matches!(
                    existing.status,
                    ExtensionStatus::Active | ExtensionStatus::RolledBack
                );
            if replacing_live {
                // Keep the live row snapshotted so activate can swap and rollback
                // can restore the previous artifact, not the candidate.
                self.repo
                    .insert_snapshot(&snapshot_of(&existing, actor))
                    .await?;
            }
            let mut updated = existing;
            updated.display_name = request.display_name;
            updated.version = request.version;
            updated.sdk_semver = request.sdk_semver;
            updated.manifest = request.manifest;
            updated.artifact_hash = request.artifact_hash;
            updated.scopes = request.scopes;
            if replacing_live {
                updated.status = ExtensionStatus::Draft;
                updated.activated_at = None;
            }
            updated.updated_at = Utc::now();
            self.repo.update(&updated).await?;
            return Ok(updated);
        }
        let now = Utc::now();
        let ext = TenantExtension {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            slug: request.slug,
            display_name: request.display_name,
            version: request.version,
            sdk_semver: request.sdk_semver,
            manifest: request.manifest,
            artifact_hash: request.artifact_hash,
            status: ExtensionStatus::Draft,
            scopes: request.scopes,
            principal_id: None,
            activated_at: None,
            created_at: now,
            updated_at: now,
        };
        self.repo.insert(&ext).await?;
        Ok(ext)
    }

    #[tracing::instrument(skip(self), err)]
    async fn activate(&self, id: Uuid, actor: &str) -> Result<TenantExtension> {
        let mut ext = self.repo.get(id).await?.ok_or(ExtensionError::NotFound)?;
        if matches!(ext.status, ExtensionStatus::Disabled) {
            return Err(ExtensionError::InvalidStatus(
                "disabled extensions cannot be activated".to_string(),
            ));
        }
        let live_already_snapshotted = matches!(ext.status, ExtensionStatus::Draft)
            && self.repo.latest_snapshot(id).await?.is_some();
        if !live_already_snapshotted {
            self.repo.insert_snapshot(&snapshot_of(&ext, actor)).await?;
        }
        ext.status = ExtensionStatus::Active;
        ext.activated_at = Some(Utc::now());
        ext.updated_at = Utc::now();
        self.repo.update(&ext).await?;
        self.repo
            .upsert_catalog(ext.org_id, catalog_json(&ext), actor)
            .await?;
        Ok(ext)
    }

    #[tracing::instrument(skip(self), err)]
    async fn rollback(&self, id: Uuid, actor: &str) -> Result<TenantExtension> {
        let mut ext = self.repo.get(id).await?.ok_or(ExtensionError::NotFound)?;
        if matches!(ext.status, ExtensionStatus::Disabled) {
            return Err(ExtensionError::InvalidStatus(
                "disabled extensions cannot be rolled back".to_string(),
            ));
        }
        let snapshot =
            self.repo.latest_snapshot(id).await?.ok_or_else(|| {
                ExtensionError::InvalidRequest("no snapshot to restore".to_string())
            })?;
        ext.version = snapshot.version;
        ext.manifest = snapshot.manifest;
        ext.artifact_hash = snapshot.artifact_hash;
        ext.status = ExtensionStatus::RolledBack;
        ext.updated_at = Utc::now();
        self.repo.update(&ext).await?;
        self.repo
            .upsert_catalog(ext.org_id, catalog_json(&ext), actor)
            .await?;
        Ok(ext)
    }

    #[tracing::instrument(skip(self), err)]
    async fn disable(&self, id: Uuid, actor: &str) -> Result<TenantExtension> {
        let mut ext = self.repo.get(id).await?.ok_or(ExtensionError::NotFound)?;
        ext.status = ExtensionStatus::Disabled;
        ext.updated_at = Utc::now();
        self.repo.update(&ext).await?;
        self.repo
            .upsert_catalog(ext.org_id, catalog_json(&ext), actor)
            .await?;
        Ok(ext)
    }
}
