//! Inbound adapters for agent identity.

pub mod axum_extractor;
pub mod axum_router;

pub use axum_extractor::AgentBearer;
