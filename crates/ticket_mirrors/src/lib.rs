//! Zendesk and Jira mirroring via the foreign-entity pattern.
//!
//! The Macro entity is the source of truth. A mirror stores a summary and
//! backlink on the external ticket; disconnecting a mirror does not change
//! anything native.

#![deny(missing_docs)]

pub mod domain;
pub mod inbound;
pub mod outbound;
