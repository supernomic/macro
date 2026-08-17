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
        _org_id: i32,
        _catalog: serde_json::Value,
        _updated_by: &str,
    ) -> Result<()> {
        Ok(())
    }
}

fn svc() -> ExtensionServiceImpl<Fake> {
    ExtensionServiceImpl::new(Fake::default())
}

fn req(hash: &str) -> RegisterExtension {
    RegisterExtension {
        slug: "acme-hooks".into(),
        display_name: "Acme".into(),
        version: "1.0.0".into(),
        sdk_semver: "2.0.3".into(),
        manifest: json!({"tools": ["ping"]}),
        artifact_hash: hash.into(),
        scopes: vec!["tool:search".into()],
    }
}

#[tokio::test]
async fn retagged_release_is_refused() {
    let svc = svc();
    svc.register(1, req("aaa")).await.unwrap();
    let err = svc.register(1, req("bbb")).await.unwrap_err();
    assert!(matches!(err, ExtensionError::RetaggedRelease { .. }));
}

#[tokio::test]
async fn activate_snapshots_then_rollback_restores() {
    let svc = svc();
    let ext = svc.register(1, req("aaa")).await.unwrap();
    let active = svc.activate(ext.id, "admin").await.unwrap();
    assert_eq!(active.status, ExtensionStatus::Active);
    let rolled = svc.rollback(ext.id, "admin").await.unwrap();
    assert_eq!(rolled.status, ExtensionStatus::RolledBack);
    assert_eq!(rolled.artifact_hash, "aaa");
}

#[tokio::test]
async fn disabled_cannot_activate() {
    let svc = svc();
    let ext = svc.register(1, req("aaa")).await.unwrap();
    svc.disable(ext.id).await.unwrap();
    let err = svc.activate(ext.id, "admin").await.unwrap_err();
    assert!(matches!(err, ExtensionError::InvalidStatus(_)));
}
