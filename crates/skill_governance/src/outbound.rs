//! Outbound adapters for skills governance.

pub mod pg_proposal_repo;
pub mod pg_skill_repo;
pub mod pg_team_membership;

pub use pg_proposal_repo::PgProposalRepo;
pub use pg_skill_repo::PgSkillRepo;
pub use pg_team_membership::PgTeamMembership;
