//! Hexagonal connectors for Okta, Iru (Kandji), and Meraki.
//!
//! Each provider maps customer vocabulary onto shared schema.org-inspired
//! graph types at this boundary, then upserts nodes/edges through the
//! entity-graph port. Secrets never live here — only a `credential_ref`
//! bound to (host, header) at an egress proxy.

#![deny(missing_docs)]

pub mod domain;
pub mod inbound;
pub mod outbound;
