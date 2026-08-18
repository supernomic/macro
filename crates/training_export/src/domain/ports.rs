//! Ports for training export.

use std::collections::HashMap;

use agent_ledger::domain::model::AgentEvent;
use agent_ledger::domain::ports::EventFilter;
use macro_uuid::Uuid;

use super::model::{ExportJob, Result, SharingMode};

/// Ledger reader (implemented by the ledger service).
pub trait LedgerReader: Send + Sync + 'static {
    /// Query events.
    fn query_events(
        &self,
        filter: EventFilter,
    ) -> impl Future<Output = Result<Vec<AgentEvent>>> + Send;
}

/// Job storage.
pub trait ExportJobRepo: Send + Sync + 'static {
    /// Insert a job.
    fn insert(&self, job: &ExportJob) -> impl Future<Output = Result<()>> + Send;

    /// Update a job.
    fn update(&self, job: &ExportJob) -> impl Future<Output = Result<()>> + Send;
}

/// Per-session training-export consent (`agent_session_consent`).
///
/// Missing rows must be omitted from the result — never treated as Full.
pub trait ConsentReader: Send + Sync + 'static {
    /// Look up sharing modes for the given sessions.
    ///
    /// Sessions with no consent row are absent from the returned map.
    fn sharing_modes(
        &self,
        session_ids: &[Uuid],
    ) -> impl Future<Output = Result<HashMap<Uuid, SharingMode>>> + Send;
}
