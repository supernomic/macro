//! Ports for ticket mirrors.

use super::model::{Result, TicketMirror, UpsertMirror};

/// Storage.
pub trait MirrorRepo: Send + Sync + 'static {
    /// Insert or update an active mirror for the native entity + provider.
    fn upsert(&self, mirror: &TicketMirror) -> impl Future<Output = Result<TicketMirror>> + Send;

    /// Fetch by id.
    fn get(
        &self,
        id: macro_uuid::Uuid,
    ) -> impl Future<Output = Result<Option<TicketMirror>>> + Send;

    /// Disconnect. Native entity is not touched.
    fn disconnect(
        &self,
        id: macro_uuid::Uuid,
    ) -> impl Future<Output = Result<Option<TicketMirror>>> + Send;

    /// Find the active mirror for a native entity + provider.
    fn find_active(
        &self,
        org_id: Option<i32>,
        native_entity_type: &str,
        native_entity_id: &str,
        provider: super::model::MirrorProvider,
    ) -> impl Future<Output = Result<Option<TicketMirror>>> + Send;
}

/// Outbound ticket API. A no-op impl is valid in tests; production posts
/// summary + backlink to Zendesk/Jira.
pub trait ExternalTicketClient: Send + Sync + 'static {
    /// Push the summary to the external ticket. Failures are returned to the
    /// caller; disconnect still works locally.
    fn push_summary(&self, mirror: &UpsertMirror) -> impl Future<Output = Result<()>> + Send;
}
