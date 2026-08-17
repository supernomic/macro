use std::sync::Mutex;

use super::*;
use crate::domain::model::{
    ExtensionSnapshot, ExtensionStatus, RegisterExtension, TenantExtension,
};
use crate::domain::ports::ExtensionRepo;
use macro_uuid::Uuid;
use serde_json::json;

#[derive(Default)]
struct Fake {
    rows: Mutex<Vec<TenantExtension>>,
    snapshots: Mutex<Vec<ExtensionSnapshot>>,
    catalogs: Mutex<Vec<(i32, serde_json::Value, String)>>,
}

impl ExtensionRepo for Fake {
    async fn insert(&self, ext: &TenantExtension) -> Result<()> {
        self.rows.lock().unwrap().push(ext.clone());
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<TenantExtension>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.id == id)
            .cloned())
    }

    async fn find_by_slug(&self, org_id: i32, slug: &str) -> Result<Option<TenantExtension>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.org_id == org_id && e.slug == slug)
            .cloned())
    }

    async fn update(&self, ext: &TenantExtension) -> Result<()> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(existing) = rows.iter_mut().find(|e| e.id == ext.id) {
            *existing = ext.clone();
        }
        Ok(())
    }

    async fn insert_snapshot(&self, snapshot: &ExtensionSnapshot) -> Result<()> {
        self.snapshots.lock().unwrap().push(snapshot.clone());
        Ok(())
    }

    async fn latest_snapshot(&self, extension_id: Uuid) -> Result<Option<ExtensionSnapshot>> {
        Ok(self
            .snapshots
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.extension_id == extension_id)
            .max_by_key(|s| s.created_at)
            .cloned())
    }

    async fn upsert_catalog(
        &self,
        org_id: i32,
        catalog: serde_json::Value,
        updated_by: &str,
    ) -> Result<()> {
        self.catalogs
            .lock()
            .unwrap()
            .push((org_id, catalog, updated_by.to_string()));
        Ok(())
    }
}

fn svc() -> ExtensionServiceImpl<Fake> {
    ExtensionServiceImpl::new(Fake::default())
}

fn req(version: &str, hash: &str) -> RegisterExtension {
    RegisterExtension {
        slug: "acme-hooks".into(),
        display_name: "Acme".into(),
        version: version.into(),
        sdk_semver: "2.0.3".into(),
        manifest: json!({"tools": ["ping"]}),
        artifact_hash: hash.into(),
        scopes: vec!["tool:search".into()],
    }
}

#[tokio::test]
async fn retagged_release_is_refused() {
    let svc = svc();
    svc.register(1, req("1.0.0", "aaa"), "admin").await.unwrap();
    let err = svc
        .register(1, req("1.0.0", "bbb"), "admin")
        .await
        .unwrap_err();
    assert!(matches!(err, ExtensionError::RetaggedRelease { .. }));
}

#[tokio::test]
async fn same_version_same_hash_is_idempotent() {
    let svc = svc();
    let first = svc.register(1, req("1.0.0", "aaa"), "admin").await.unwrap();
    let second = svc.register(1, req("1.0.0", "aaa"), "admin").await.unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.artifact_hash, "aaa");
}

#[tokio::test]
async fn activate_snapshots_then_rollback_restores() {
    let svc = svc();
    let ext = svc.register(1, req("1.0.0", "aaa"), "admin").await.unwrap();
    let active = svc.activate(ext.id, "admin").await.unwrap();
    assert_eq!(active.status, ExtensionStatus::Active);
    let rolled = svc.rollback(ext.id, "admin").await.unwrap();
    assert_eq!(rolled.status, ExtensionStatus::RolledBack);
    assert_eq!(rolled.artifact_hash, "aaa");
}

#[tokio::test]
async fn activate_swaps_new_version_and_rollback_restores_live_snapshot() {
    let svc = svc();
    let ext = svc.register(1, req("1.0.0", "aaa"), "admin").await.unwrap();
    svc.activate(ext.id, "admin").await.unwrap();
    let candidate = svc.register(1, req("1.1.0", "bbb"), "admin").await.unwrap();
    assert_eq!(candidate.id, ext.id);
    assert_eq!(candidate.status, ExtensionStatus::Draft);
    assert_eq!(candidate.artifact_hash, "bbb");
    let active = svc.activate(ext.id, "admin").await.unwrap();
    assert_eq!(active.status, ExtensionStatus::Active);
    assert_eq!(active.artifact_hash, "bbb");
    let rolled = svc.rollback(ext.id, "admin").await.unwrap();
    assert_eq!(rolled.status, ExtensionStatus::RolledBack);
    assert_eq!(rolled.artifact_hash, "aaa");
    assert_eq!(rolled.version, "1.0.0");
}

#[tokio::test]
async fn disabled_cannot_activate() {
    let svc = svc();
    let ext = svc.register(1, req("1.0.0", "aaa"), "admin").await.unwrap();
    let disabled = svc.disable(ext.id, "admin").await.unwrap();
    assert_eq!(disabled.status, ExtensionStatus::Disabled);
    let err = svc.activate(ext.id, "admin").await.unwrap_err();
    assert!(matches!(err, ExtensionError::InvalidStatus(_)));
}

#[tokio::test]
async fn disable_is_a_kill_switch() {
    let svc = svc();
    let ext = svc.register(1, req("1.0.0", "aaa"), "admin").await.unwrap();
    svc.activate(ext.id, "admin").await.unwrap();
    let disabled = svc.disable(ext.id, "ops").await.unwrap();
    assert_eq!(disabled.status, ExtensionStatus::Disabled);
    assert!(matches!(
        svc.activate(ext.id, "admin").await.unwrap_err(),
        ExtensionError::InvalidStatus(_)
    ));
    assert!(matches!(
        svc.rollback(ext.id, "admin").await.unwrap_err(),
        ExtensionError::InvalidStatus(_)
    ));
    let catalogs = svc.repo.catalogs.lock().unwrap();
    let last = catalogs.last().unwrap();
    assert_eq!(last.0, 1);
    assert_eq!(last.2, "ops");
    assert_eq!(last.1["extensions"][0]["status"], "disabled");
    assert!(matches!(
        svc.register(1, req("1.2.0", "ccc"), "admin")
            .await
            .unwrap_err(),
        ExtensionError::InvalidStatus(_)
    ));
}
