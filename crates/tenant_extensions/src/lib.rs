//! Per-tenant Flue-native extensions (bb pattern).
//!
//! An extension is a capability package (hooks + tools) + OKF skill doc +
//! UI slots, capability-scoped via the extension principal's API token.
//! Activation is a candidate-set swap with snapshot rollback; re-tagged
//! releases with a different artifact hash are refused.

#![deny(missing_docs)]

pub mod domain;
pub mod inbound;
pub mod outbound;
