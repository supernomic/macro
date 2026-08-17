//! Session feedback sidecar: editable ratings and sharing-consent overlay.
//!
//! Immutable `feedback/record` events live on the session ledger. This crate
//! stores the *editable* rating overlay (`agent_message_ratings`) and the
//! per-session training-export consent (`agent_session_consent`).

#![deny(missing_docs)]

pub mod domain;
pub mod inbound;
pub mod outbound;
