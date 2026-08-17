//! Ticket-mirror domain service.

#[cfg(test)]
mod test;

use chrono::Utc;
use macro_uuid::Uuid;

use super::model::{MirrorError, Result, TicketMirror, UpsertMirror};
use super::ports::{ExternalTicketClient, MirrorRepo};

/// Domain service.
pub trait MirrorService: Send + Sync + 'static {
    /// Upsert a mirror and push the summary outward. Macro remains source of truth.
    fn upsert(
        &self,
        org_id: Option<i32>,
        request: UpsertMirror,
    ) -> impl Future<Output = Result<TicketMirror>> + Send;

    /// Disconnect a mirror. Native entity is unchanged.
    fn disconnect(&self, id: Uuid) -> impl Future<Output = Result<TicketMirror>> + Send;

    /// Fetch a mirror.
    fn get(&self, id: Uuid) -> impl Future<Output = Result<TicketMirror>> + Send;
}

/// Concrete service.
#[derive(Debug, Clone)]
pub struct MirrorServiceImpl<R, C> {
    repo: R,
    client: C,
}

impl<R: MirrorRepo, C: ExternalTicketClient> MirrorServiceImpl<R, C> {
    /// Build over storage + external client.
    pub fn new(repo: R, client: C) -> Self {
        Self { repo, client }
    }
}

impl<R: MirrorRepo, C: ExternalTicketClient> MirrorService for MirrorServiceImpl<R, C> {
    #[tracing::instrument(skip(self, request), err)]
    async fn upsert(&self, org_id: Option<i32>, request: UpsertMirror) -> Result<TicketMirror> {
        if request.native_entity_id.trim().is_empty() || request.foreign_id.trim().is_empty() {
            return Err(MirrorError::InvalidRequest(
                "native_entity_id and foreign_id are required".to_string(),
            ));
        }
        self.client.push_summary(&request).await?;
        let now = Utc::now();
        let row = TicketMirror {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            provider: request.provider,
            native_entity_type: request.native_entity_type,
            native_entity_id: request.native_entity_id,
            foreign_id: request.foreign_id,
            foreign_url: request.foreign_url,
            summary: request.summary,
            status: request.status,
            last_mirrored_at: now,
            disconnected_at: None,
        };
        self.repo.upsert(&row).await
    }

    #[tracing::instrument(skip(self), err)]
    async fn disconnect(&self, id: Uuid) -> Result<TicketMirror> {
        self.repo.disconnect(id).await?.ok_or(MirrorError::NotFound)
    }

    #[tracing::instrument(skip(self), err)]
    async fn get(&self, id: Uuid) -> Result<TicketMirror> {
        self.repo.get(id).await?.ok_or(MirrorError::NotFound)
    }
}
