//! Outbound adapters for approval gates.

pub mod http_callback_client;
pub mod noop_notifier;
pub mod pg_approval_repo;
pub mod pg_policy_repo;
pub mod pg_team_membership;

pub use http_callback_client::HttpCallbackClient;
pub use noop_notifier::NoopNotifier;
pub use pg_approval_repo::PgApprovalRepo;
pub use pg_policy_repo::PgPolicyRepo;
pub use pg_team_membership::PgTeamMembership;
