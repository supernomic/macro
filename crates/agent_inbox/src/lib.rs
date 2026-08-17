//! Unified agent review inbox: one `GET /inbox/mine` over escalations,
//! approval gates, and skill proposals.
//!
//! Each source already has a personal `/…/mine` list. This crate is a
//! facade that composes those lists into a single, newest-first card feed
//! for Agent Review. Decide/claim stay on the source services.

#![deny(missing_docs)]

pub mod domain;
pub mod inbound;
