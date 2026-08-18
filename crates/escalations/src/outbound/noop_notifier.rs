//! No-op notifier for deployments where the notification pipeline is not
//! wired yet. Assignment visibility still comes from the inbox queries.

use crate::domain::model::{Escalation, Result};
use crate::domain::ports::EscalationNotifier;

/// Notifier that records the intent in logs and does nothing else.
#[derive(Debug, Clone, Default)]
pub struct NoopNotifier;

impl EscalationNotifier for NoopNotifier {
    async fn notify_assigned(
        &self,
        escalation: &Escalation,
        recipient_user_ids: &[String],
    ) -> Result<()> {
        tracing::info!(
            escalation_id = %escalation.id,
            recipients = recipient_user_ids.len(),
            "escalation assignment (notifications not wired; inbox only)"
        );
        Ok(())
    }
}
