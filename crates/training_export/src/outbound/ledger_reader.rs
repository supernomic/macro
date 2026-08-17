//! Ledger reader adapter over the ledger domain service.

use agent_ledger::domain::model::AgentEvent;
use agent_ledger::domain::ports::{EventFilter, LedgerService};

use crate::domain::model::{ExportError, Result};
use crate::domain::ports::LedgerReader;

/// Wraps a [`LedgerService`] as a [`LedgerReader`].
#[derive(Debug, Clone)]
pub struct LedgerServiceReader<S> {
    inner: S,
}

impl<S: LedgerService> LedgerServiceReader<S> {
    /// Build over a ledger service.
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S: LedgerService> LedgerReader for LedgerServiceReader<S> {
    async fn query_events(&self, filter: EventFilter) -> Result<Vec<AgentEvent>> {
        self.inner
            .query_events(filter)
            .await
            .map_err(|e| ExportError::InvalidRequest(e.to_string()))
    }
}
