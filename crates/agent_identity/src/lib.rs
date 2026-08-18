//! Agent identity: first-class agent principals and scoped API tokens.
//!
//! Agents are principals in Macro, distinct from users. Every agent action
//! crosses Macro's API boundary carrying a scoped bearer token
//! (`mat_<token_id>.<secret>`), so capability allowlists and tenancy are
//! enforced server-side at the domain boundary — not by runtime-side
//! configuration that a prompt injection could bypass.
//!
//! Scopes are colon-separated capability strings (e.g. `tool:search`,
//! `ledger:append`, `api:documents:read`) with a trailing `*` wildcard
//! segment (`tool:*`). Domain tool allowlists and per-tenant extension
//! capabilities are expressed as scope sets on the token.
#![deny(missing_docs)]

pub mod domain;
pub mod inbound;
pub mod outbound;
