//! Approval gates for agent tool calls.
//!
//! Every gated tool call consults Macro before running:
//! `allow` / `require_approval` / `deny` per agent + tool, evaluated from a
//! built-in policy floor that org policies can only tighten (never loosen).
//! `require_approval` pauses the call behind a pending approval request
//! routed to an approver (user or team), actionable from the Macro inbox
//! and Slack; the decision resumes the agent runtime via callback and is
//! recorded both here (audited transitions) and in the session ledger
//! (`approval/requested`, `approval/decided` events written by the
//! runtime's tool layer).

#![deny(missing_docs)]

pub mod domain;
pub mod inbound;
pub mod outbound;
