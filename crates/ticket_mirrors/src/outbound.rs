//! Outbound adapters for ticket mirrors.

pub mod noop_ticket_client;
pub mod pg_mirror_repo;

pub use noop_ticket_client::NoopTicketClient;
pub use pg_mirror_repo::PgMirrorRepo;
