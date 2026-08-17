use std::sync::Mutex;

use super::*;
use crate::domain::model::{MirrorProvider, TicketMirror, UpsertMirror};
use crate::domain::ports::{ExternalTicketClient, MirrorRepo};
use chrono::Utc;
use macro_uuid::Uuid;

#[derive(Default)]
struct FakeRepo {
    rows: Mutex<Vec<TicketMirror>>,
}

impl MirrorRepo for FakeRepo {
    async fn upsert(&self, mirror: &TicketMirror) -> Result<TicketMirror> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(existing) = rows.iter_mut().find(|m| {
            m.org_id == mirror.org_id
                && m.provider == mirror.provider
                && m.native_entity_type == mirror.native_entity_type
                && m.native_entity_id == mirror.native_entity_id
                && m.disconnected_at.is_none()
        }) {
            existing.summary = mirror.summary.clone();
            existing.status = mirror.status.clone();
            existing.foreign_url = mirror.foreign_url.clone();
            existing.last_mirrored_at = mirror.last_mirrored_at;
            return Ok(existing.clone());
        }
        rows.push(mirror.clone());
        Ok(mirror.clone())
    }

    async fn get(&self, id: Uuid) -> Result<Option<TicketMirror>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.id == id)
            .cloned())
    }

    async fn disconnect(&self, id: Uuid) -> Result<Option<TicketMirror>> {
        let mut rows = self.rows.lock().unwrap();
        let Some(m) = rows.iter_mut().find(|m| m.id == id) else {
            return Ok(None);
        };
        m.disconnected_at = Some(Utc::now());
        Ok(Some(m.clone()))
    }

    async fn find_active(
        &self,
        org_id: Option<i32>,
        native_entity_type: &str,
        native_entity_id: &str,
        provider: MirrorProvider,
    ) -> Result<Option<TicketMirror>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|m| {
                m.org_id == org_id
                    && m.provider == provider
                    && m.native_entity_type == native_entity_type
                    && m.native_entity_id == native_entity_id
                    && m.disconnected_at.is_none()
            })
            .cloned())
    }
}

struct NoopClient;

impl ExternalTicketClient for NoopClient {
    async fn push_summary(&self, _mirror: &UpsertMirror) -> Result<()> {
        Ok(())
    }
}

fn svc() -> MirrorServiceImpl<FakeRepo, NoopClient> {
    MirrorServiceImpl::new(FakeRepo::default(), NoopClient)
}

#[tokio::test]
async fn upsert_then_disconnect_leaves_native_id_intact() {
    let svc = svc();
    let mirror = svc
        .upsert(
            Some(1),
            UpsertMirror {
                provider: MirrorProvider::Zendesk,
                native_entity_type: "escalation".into(),
                native_entity_id: "esc-1".into(),
                foreign_id: "ZD-99".into(),
                foreign_url: Some("https://example.zendesk.com/99".into()),
                summary: "Need laptop".into(),
                status: "open".into(),
            },
        )
        .await
        .unwrap();
    let disconnected = svc.disconnect(mirror.id).await.unwrap();
    assert!(disconnected.disconnected_at.is_some());
    assert_eq!(disconnected.native_entity_id, "esc-1");
}

#[tokio::test]
async fn empty_foreign_id_rejected() {
    let svc = svc();
    let err = svc
        .upsert(
            Some(1),
            UpsertMirror {
                provider: MirrorProvider::Jira,
                native_entity_type: "task".into(),
                native_entity_id: "t-1".into(),
                foreign_id: " ".into(),
                foreign_url: None,
                summary: "x".into(),
                status: "open".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, MirrorError::InvalidRequest(_)));
}
