//! Training-export pipeline: three projections of one ledger.
//!
//! - `model_history` — what the model actually saw after compaction
//! - `human_transcript` — user/assistant messages only
//! - `training_export` — byte-exact reconstructable trajectory, gated by
//!   sharing mode (`full` / `feedback_only` / `disabled`)

#![deny(missing_docs)]

pub mod domain;
pub mod inbound;
pub mod outbound;
