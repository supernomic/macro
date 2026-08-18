//! Typed entity/context graph and OKF knowledge corpus.
//!
//! Generalizes `entity_mentions` / `foreign_entity` into typed nodes and
//! relationships (employee ⟷ device ⟷ application). Knowledge documents
//! carry OKF provenance; human-authored bodies are never overwritten.

#![deny(missing_docs)]

pub mod domain;
pub mod inbound;
pub mod outbound;
