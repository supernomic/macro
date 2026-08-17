use std::sync::{Arc, Mutex};

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
            existing.foreign_id = mirror.foreign_id.clone();
            existing.foreign_url = mirror.foreign_url.clone();
            existing.summary = mirror.summary.clone();
            existing.status = mirror.status.clone();
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
        let Some(m) = rows
            .iter_mut()
            .find(|m| m.id == id && m.disconnected_at.is_none())
        else {
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

/// Records `push_summary` calls so the port is tested without Zendesk/Jira HTTP.
#[derive(Clone, Default)]
struct RecordingClient {
    pushed: Arc<Mutex<Vec<UpsertMirror>>>,
}

impl ExternalTicketClient for RecordingClient {
    async fn push_summary(&self, mirror: &UpsertMirror) -> Result<()> {
        self.pushed.lock().unwrap().push(mirror.clone());
        Ok(())
    }
}

fn svc() -> (
    MirrorServiceImpl<FakeRepo, RecordingClient>,
    RecordingClient,
) {
    let client = RecordingClient::default();
    (
        MirrorServiceImpl::new(FakeRepo::default(), client.clone()),
        client,
    )
}

fn zendesk_upsert(foreign_id: &str, summary: &str) -> UpsertMirror {
    UpsertMirror {
        provider: MirrorProvider::Zendesk,
        native_entity_type: "escalation".into(),
        native_entity_id: "esc-1".into(),
        foreign_id: foreign_id.into(),
        foreign_url: Some("https://example.zendesk.com/99".into()),
        summary: summary.into(),
        status: "open".into(),
    }
}

#[tokio::test]
async fn upsert_then_disconnect_leaves_native_id_intact() {
    let (svc, client) = svc();
    let mirror = svc
        .upsert(Some(1), zendesk_upsert("ZD-99", "Need laptop"))
        .await
        .unwrap();
    let disconnected = svc.disconnect(mirror.id).await.unwrap();
    assert!(disconnected.disconnected_at.is_some());
    assert_eq!(disconnected.native_entity_id, "esc-1");
    assert_eq!(disconnected.native_entity_type, "escalation");
    assert_eq!(client.pushed.lock().unwrap().len(), 1);
    let stored = svc.get(mirror.id).await.unwrap();
    assert_eq!(stored.native_entity_id, "esc-1");
}

#[tokio::test]
async fn upsert_reuses_existing_id_and_pushes_summary() {
    let (svc, client) = svc();
    let first = svc
        .upsert(Some(1), zendesk_upsert("ZD-99", "Need laptop"))
        .await
        .unwrap();
    let second = svc
        .upsert(Some(1), zendesk_upsert("ZD-99", "Need laptop yesterday"))
        .await
        .unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.summary, "Need laptop yesterday");
    assert_eq!(second.native_entity_id, "esc-1");
    let pushed = client.pushed.lock().unwrap();
    assert_eq!(pushed.len(), 2);
    assert_eq!(pushed[0].foreign_id, "ZD-99");
    assert_eq!(pushed[1].summary, "Need laptop yesterday");
}

#[tokio::test]
async fn disconnect_does_not_push_and_reconnect_mints_a_new_row() {
    let (svc, client) = svc();
    let first = svc
        .upsert(Some(1), zendesk_upsert("ZD-99", "Need laptop"))
        .await
        .unwrap();
    svc.disconnect(first.id).await.unwrap();
    let reconnected = svc
        .upsert(Some(1), zendesk_upsert("ZD-100", "Need laptop"))
        .await
        .unwrap();
    assert_ne!(first.id, reconnected.id);
    assert_eq!(reconnected.native_entity_id, "esc-1");
    let original = svc.get(first.id).await.unwrap();
    assert!(original.disconnected_at.is_some());
    assert_eq!(original.native_entity_id, "esc-1");
    assert_eq!(client.pushed.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn empty_foreign_id_rejected() {
    let (svc, client) = svc();
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
    assert!(client.pushed.lock().unwrap().is_empty());
}
