//! Escalations: agent → human-expert handoff with configurable routing.
//!
//! When an agent cannot resolve a request it creates an **escalation**: a
//! durable entity linking the agent session to a human owner (a specific
//! expert or a team queue). Routing rules per domain decide who gets it;
//! team-routed items use claim semantics (first claim owns it) or
//! round-robin assignment. Resolving an escalation notifies the agent
//! runtime through a callback so the originating conversation resumes.
//!
//! Hexagonal layout: `domain` holds the model, ports, service, and the
//! agent-facing facade; `inbound` the axum router; `outbound` the Postgres
//! repositories and the HTTP callback client.
#![deny(missing_docs)]

pub mod domain;
pub mod inbound;
pub mod outbound;
