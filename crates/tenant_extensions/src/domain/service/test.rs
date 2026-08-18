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

    async fn get_catalog(&self, org_id: i32) -> Result<Option<serde_json::Value>> {
        Ok(self
            .catalogs
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(id, _, _)| *id == org_id)
            .map(|(_, catalog, _)| catalog.clone()))
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

    async fn merge_catalog(&self, ext: &TenantExtension, actor: &str) -> Result<()> {
        let mut catalogs = self.catalogs.lock().unwrap();
        let existing = catalogs
            .iter()
            .rev()
            .find(|(id, _, _)| *id == ext.org_id)
            .map(|(_, catalog, _)| catalog.clone());
        catalogs.push((ext.org_id, catalog_json(existing, ext), actor.to_string()));
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

fn req_slug(slug: &str, version: &str, hash: &str) -> RegisterExtension {
    RegisterExtension {
        slug: slug.into(),
        display_name: slug.into(),
        version: version.into(),
        sdk_semver: "2.0.3".into(),
        manifest: json!({"tools": ["ping"]}),
        artifact_hash: hash.into(),
        scopes: vec!["tool:search".into()],
    }
}

fn catalog_by_slug<'a>(catalog: &'a serde_json::Value, slug: &str) -> &'a serde_json::Value {
    catalog["extensions"]
        .as_array()
        .expect("extensions array")
        .iter()
        .find(|entry| entry["slug"] == slug)
        .unwrap_or_else(|| panic!("missing slug {slug}"))
}

#[tokio::test]
async fn register_merges_catalog_by_slug() {
    let svc = svc();
    svc.register(1, req_slug("ext-a", "1.0.0", "aaa"), "admin")
        .await
        .unwrap();
    svc.register(1, req_slug("ext-b", "2.0.0", "bbb"), "admin")
        .await
        .unwrap();
    let catalog = svc.repo.get_catalog(1).await.unwrap().unwrap();
    let slugs: Vec<&str> = catalog["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, vec!["ext-a", "ext-b"]);
}

#[tokio::test]
async fn reregister_updates_only_that_slug() {
    let svc = svc();
    svc.register(1, req_slug("ext-a", "1.0.0", "aaa"), "admin")
        .await
        .unwrap();
    svc.register(1, req_slug("ext-b", "2.0.0", "bbb"), "admin")
        .await
        .unwrap();
    svc.register(1, req_slug("ext-a", "1.1.0", "ccc"), "admin")
        .await
        .unwrap();
    let catalog = svc.repo.get_catalog(1).await.unwrap().unwrap();
    assert_eq!(catalog["extensions"].as_array().unwrap().len(), 2);
    let a = catalog_by_slug(&catalog, "ext-a");
    let b = catalog_by_slug(&catalog, "ext-b");
    assert_eq!(a["version"], "1.1.0");
    assert_eq!(a["artifact_hash"], "ccc");
    assert_eq!(a["status"], "draft");
    assert_eq!(a["enabled"], json!(true));
    assert_eq!(b["version"], "2.0.0");
    assert_eq!(b["artifact_hash"], "bbb");
    assert_eq!(b["status"], "draft");
}

#[tokio::test]
async fn disable_keeps_slug_marked_disabled() {
    let svc = svc();
    let a = svc
        .register(1, req_slug("ext-a", "1.0.0", "aaa"), "admin")
        .await
        .unwrap();
    svc.register(1, req_slug("ext-b", "2.0.0", "bbb"), "admin")
        .await
        .unwrap();
    svc.activate(a.id, "admin").await.unwrap();
    svc.disable(a.id, "ops").await.unwrap();
    let catalog = svc.repo.get_catalog(1).await.unwrap().unwrap();
    assert_eq!(catalog["extensions"].as_array().unwrap().len(), 2);
    let a = catalog_by_slug(&catalog, "ext-a");
    let b = catalog_by_slug(&catalog, "ext-b");
    assert_eq!(a["status"], "disabled");
    assert_eq!(a["enabled"], json!(false));
    assert_eq!(b["status"], "draft");
    assert_eq!(b["enabled"], json!(true));
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
    {
        let catalogs = svc.repo.catalogs.lock().unwrap();
        let last = catalogs.last().unwrap();
        assert_eq!(last.0, 1);
        assert_eq!(last.2, "ops");
        assert_eq!(last.1["extensions"][0]["status"], "disabled");
        assert_eq!(last.1["extensions"][0]["enabled"], json!(false));
    }
    assert!(matches!(
        svc.register(1, req("1.2.0", "ccc"), "admin")
            .await
            .unwrap_err(),
        ExtensionError::InvalidStatus(_)
    ));
}
