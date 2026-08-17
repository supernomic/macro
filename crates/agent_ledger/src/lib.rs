//! Append-only session ledger for the agent platform.
//!
//! The ledger is the canonical, customer-owned record of everything an agent
//! saw and did. It follows the "model-visible means logged" axiom: anything
//! that reaches a model request must be reconstructable from the ledger, so
//! every trace is usable as audit evidence and as training data.
//!
//! Design notes:
//! - Events are typed ([`domain::model::AgentEventPayload`]) and stored as an
//!   append-only sequence per session with a per-session hash chain for
//!   tamper evidence.
//! - The event vocabulary is adapted from DeepSeek Harness' session log and
//!   Block Buzz's unified audit stream: human messages, agent tool calls,
//!   approvals, escalations, skill injections, and feedback are all the same
//!   kind of record.
//! - The ledger also owns the session mapping between runtime conversation
//!   ids (Flue), Macro session entities, and external threads (Slack, email).
#![deny(missing_docs)]

pub mod domain;
pub mod outbound;
