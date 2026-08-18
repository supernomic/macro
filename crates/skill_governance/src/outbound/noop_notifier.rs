//! No-op skill-proposal notifier for deployments where notifications
//! are not wired yet.

use crate::domain::model::{Result, SkillProposal};
use crate::domain::ports::SkillNotifier;

/// Drops notifications on the floor (they are still visible in the inbox
/// views served by the API).
#[derive(Debug, Clone, Default)]
pub struct NoopNotifier;

impl SkillNotifier for NoopNotifier {
    async fn notify_assigned(
        &self,
        _proposal: &SkillProposal,
        _recipient_user_ids: &[String],
    ) -> Result<()> {
        Ok(())
    }
}
