//! No-op external ticket client. Production replaces this with Zendesk/Jira
//! HTTP adapters that post summary + backlink.

use crate::domain::model::{Result, UpsertMirror};
use crate::domain::ports::ExternalTicketClient;

/// No-op client used until provider HTTP adapters are wired.
#[derive(Debug, Clone, Default)]
pub struct NoopTicketClient;

impl ExternalTicketClient for NoopTicketClient {
    async fn push_summary(&self, _mirror: &UpsertMirror) -> Result<()> {
        Ok(())
    }
}
