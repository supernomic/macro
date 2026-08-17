//! Skills governance: scoped skills, OKF provenance, trust tiers, staged
//! proposals with inbox review, snapshots, rollback, and eval-gated
//! promotion. Flue agents consume the list/get API and emit `skill/injected`
//! ledger events when a skill is mounted.
//!
//! The existing `skills` crate remains the document-search toolset over
//! skill *documents*. This crate is the system of record for governed
//! agent skills (user / team / org / platform).

#![deny(missing_docs)]

pub mod domain;
pub mod inbound;
pub mod outbound;
