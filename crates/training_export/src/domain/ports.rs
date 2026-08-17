//! Ports for training export.

use agent_ledger::domain::model::AgentEvent;
use agent_ledger::domain::ports::EventFilter;

use super::model::{ExportJob, Result};

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
