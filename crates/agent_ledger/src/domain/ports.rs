//! Ports for the session ledger.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;

use super::model::{
    Actor, AgentEvent, ExternalThreadKind, NewAgentEvent, Result, SessionMapping, SessionOutcome,
};

/// The current head of a session's hash chain.
#[derive(Debug, Clone)]
pub struct ChainHead {
    /// Seq of the last event in the chain.
    pub seq: i64,
    /// Hash of the last event in the chain.
    pub hash: Vec<u8>,
}

/// Filters for querying events across sessions (audit / export path).
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// Restrict to a session.
    pub session_id: Option<Uuid>,
    /// Restrict to an organization.
    pub org_id: Option<i32>,
    /// Restrict to event types (stored discriminants like `tool/call`).
    pub event_types: Vec<String>,
    /// Restrict to an actor id.
    pub actor_id: Option<String>,
    /// Events at or after this instant.
    pub occurred_after: Option<DateTime<Utc>>,
    /// Events before this instant.
    pub occurred_before: Option<DateTime<Utc>>,
    /// Maximum rows to return.
    pub limit: i64,
}

/// A fully prepared event row ready for insertion (seq and hashes already
/// assigned by the service).
#[derive(Debug, Clone)]
pub struct PreparedEvent {
    /// The stored event.
    pub event: AgentEvent,
    /// Serialized payload JSON (adjacently tagged), stored as `data`.
    pub payload_json: serde_json::Value,
    /// Stored discriminant for the indexed `event_type` column.
    pub event_type: &'static str,
}

/// Storage port for ledger events.
pub trait LedgerRepo: Send + Sync + 'static {
    /// Read the chain head for a session, if the session has events.
    fn chain_head(
        &self,
        session_id: Uuid,
    ) -> impl Future<Output = Result<Option<ChainHead>>> + Send;

    /// Insert a contiguous batch of prepared events. Must fail with
    /// [`super::model::LedgerError::ChainConflict`] if any `(session, seq)`
    /// already exists, so the service can re-read the head and retry.
    fn insert_events(&self, events: Vec<PreparedEvent>) -> impl Future<Output = Result<()>> + Send;

    /// List events of one session in seq order, starting at `from_seq`.
    fn list_session_events(
        &self,
        session_id: Uuid,
        from_seq: i64,
        limit: i64,
    ) -> impl Future<Output = Result<Vec<AgentEvent>>> + Send;

    /// Query events across sessions for audit and export.
    fn query_events(
        &self,
        filter: &EventFilter,
    ) -> impl Future<Output = Result<Vec<AgentEvent>>> + Send;

    /// Upsert the derived outcome of a session.
    fn upsert_outcome(
        &self,
        session_id: Uuid,
        outcome: &SessionOutcome,
        summary: Option<&str>,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// Storage port for session mappings.
pub trait SessionMappingRepo: Send + Sync + 'static {
    /// Create a mapping (new session). Fails if the runtime conversation id
    /// is already mapped.
    fn create_mapping(&self, mapping: &SessionMapping) -> impl Future<Output = Result<()>> + Send;

    /// Look up by runtime conversation id.
    fn find_by_runtime_conversation(
        &self,
        runtime_conversation_id: &str,
    ) -> impl Future<Output = Result<Option<SessionMapping>>> + Send;

    /// Look up by external thread key.
    fn find_by_external_thread(
        &self,
        kind: &ExternalThreadKind,
        key: &str,
    ) -> impl Future<Output = Result<Option<SessionMapping>>> + Send;

    /// Look up by Macro session id.
    fn find_by_session(
        &self,
        session_id: Uuid,
    ) -> impl Future<Output = Result<Option<SessionMapping>>> + Send;
}

/// Domain service exposed to inbound adapters.
pub trait LedgerService: Send + Sync + 'static {
    /// Append a batch of events to a session, assigning seqs and extending
    /// the hash chain. Returns the stored events.
    fn append_events(
        &self,
        session_id: Uuid,
        org_id: Option<i32>,
        events: Vec<NewAgentEvent>,
    ) -> impl Future<Output = Result<Vec<AgentEvent>>> + Send;

    /// Replay a session's events in order.
    fn list_session_events(
        &self,
        session_id: Uuid,
        from_seq: i64,
        limit: i64,
    ) -> impl Future<Output = Result<Vec<AgentEvent>>> + Send;

    /// Query events across sessions (audit / agent self-query path).
    fn query_events(
        &self,
        filter: EventFilter,
    ) -> impl Future<Output = Result<Vec<AgentEvent>>> + Send;

    /// Verify a session's hash chain, returning the first broken seq if the
    /// chain does not verify.
    fn verify_chain(&self, session_id: Uuid) -> impl Future<Output = Result<Option<i64>>> + Send;

    /// Record the derived outcome of a session.
    fn record_outcome(
        &self,
        session_id: Uuid,
        outcome: SessionOutcome,
        summary: Option<String>,
        actor: Actor,
    ) -> impl Future<Output = Result<()>> + Send;
}
