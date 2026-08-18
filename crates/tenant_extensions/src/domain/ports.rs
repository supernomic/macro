//! Ports for tenant extensions.

use super::model::{ExtensionSnapshot, Result, TenantExtension};

/// Storage.
pub trait ExtensionRepo: Send + Sync + 'static {
    /// Insert a new extension.
    fn insert(&self, ext: &TenantExtension) -> impl Future<Output = Result<()>> + Send;

    /// Fetch by id.
    fn get(
        &self,
        id: macro_uuid::Uuid,
    ) -> impl Future<Output = Result<Option<TenantExtension>>> + Send;

    /// Fetch by org + slug.
    fn find_by_slug(
        &self,
        org_id: i32,
        slug: &str,
    ) -> impl Future<Output = Result<Option<TenantExtension>>> + Send;

    /// Persist an updated row.
    fn update(&self, ext: &TenantExtension) -> impl Future<Output = Result<()>> + Send;

    /// Insert a snapshot.
    fn insert_snapshot(
        &self,
        snapshot: &ExtensionSnapshot,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Latest snapshot for an extension.
    fn latest_snapshot(
        &self,
        extension_id: macro_uuid::Uuid,
    ) -> impl Future<Output = Result<Option<ExtensionSnapshot>>> + Send;

    /// Load the tenant catalog JSON, if any.
    fn get_catalog(
        &self,
        org_id: i32,
    ) -> impl Future<Output = Result<Option<serde_json::Value>>> + Send;

    /// Replace the tenant catalog JSON.
    fn upsert_catalog(
        &self,
        org_id: i32,
        catalog: serde_json::Value,
        updated_by: &str,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Merge `ext` into the org catalog by slug under a row lock.
    fn merge_catalog(
        &self,
        ext: &TenantExtension,
        actor: &str,
    ) -> impl Future<Output = Result<()>> + Send;
}
