//! Outbound adapters for the feedback sidecar.

pub mod pg_consent_repo;
pub mod pg_rating_repo;

pub use pg_consent_repo::PgConsentRepo;
pub use pg_rating_repo::PgRatingRepo;
