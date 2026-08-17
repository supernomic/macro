//! No-op approval notifier for deployments where notifications are not
//! wired yet.

use crate::domain::model::{ApprovalRequest, Result};
use crate::domain::ports::ApprovalNotifier;

/// Drops notifications on the floor (they are still visible in the inbox
/// views served by the API).
#[derive(Debug, Clone, Default)]
pub struct NoopNotifier;

impl ApprovalNotifier for NoopNotifier {
    async fn notify_assigned(
        &self,
        _request: &ApprovalRequest,
        _recipient_user_ids: &[String],
    ) -> Result<()> {
        Ok(())
    }
}
