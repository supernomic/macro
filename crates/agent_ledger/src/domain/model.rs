//! Ledger event vocabulary and envelope.
//!
//! Adapted from DeepSeek Harness' session event model: the session is an
//! append-only log of typed events; model-visible request state (rendered
//! system prompt, tool schemas, sampling config) is snapshotted in-log so
//! every request is reconstructable byte-exactly from the ledger.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Who performed / produced an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    /// A human user (actor id is a Macro user id).
    User,
    /// An agent principal (actor id is an `agent_principals.id`).
    Agent,
    /// Platform machinery (schedulers, compaction, crash recovery).
    System,
}

impl ActorKind {
    /// Stable string used for storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorKind::User => "user",
            ActorKind::Agent => "agent",
            ActorKind::System => "system",
        }
    }

    /// Parse from the stored string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "user" => Some(ActorKind::User),
            "agent" => Some(ActorKind::Agent),
            "system" => Some(ActorKind::System),
            _ => None,
        }
    }
}

/// The actor attached to a ledger event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    /// The kind of actor.
    pub kind: ActorKind,
    /// Identifier of the actor (user id, agent principal id, or a system
    /// component name such as `"compaction"`).
    pub id: String,
}

/// Why a turn ended. Typed outcome labels are built into the schema so
/// trajectory-level outcome signal is available for evals and training
/// export without re-parsing transcripts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnEndReason {
    /// The agent finished the turn normally.
    Completed,
    /// The turn failed with a model/provider error.
    Error {
        /// Human-readable description of the failure.
        message: String,
    },
    /// The model hit its output token limit.
    MaxTokens,
    /// The turn was cancelled by a user or the platform.
    Aborted {
        /// What caused the abort.
        cause: String,
    },
    /// The agent handed off to a human expert and is durably waiting.
    Escalated {
        /// The escalation this turn ended into.
        escalation_id: Uuid,
    },
    /// Synthetic marker written by crash recovery for orphaned turns.
    Interrupted,
}

/// Where an injected user-role message came from. Distinguishes real human
/// prompts from platform-injected context so training export can tell them
/// apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserMessageSource {
    /// A human typed this.
    Human,
    /// Platform-injected context (file notices, instructions, memory).
    InjectedContext,
    /// An expert's reply to an escalation, resuming the session.
    EscalationReply,
    /// A scheduled / cron trigger.
    Schedule,
    /// A message from another agent.
    Agent,
}

/// Token usage attached to an assistant message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens consumed by the request.
    pub input_tokens: i64,
    /// Output tokens produced by the response.
    pub output_tokens: i64,
    /// Cached input tokens, when the provider reports them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<i64>,
}

/// A snapshot of everything that shapes a model request. Logged on session
/// init and whenever any field changes, so prompt/skill/tool changes are
/// versioned inside the trajectory data itself ("model-visible means
/// logged").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestHeader {
    /// The exact rendered system prompt text sent to the model.
    pub rendered_system_prompt: String,
    /// JSON schemas of every tool exposed in the request.
    pub tool_schemas: serde_json::Value,
    /// Model provider (e.g. `anthropic`).
    pub provider: String,
    /// Model identifier (e.g. `claude-opus-4-8`).
    pub model: String,
    /// Sampling configuration (temperature, top_p, max_tokens, ...).
    pub sampling: serde_json::Value,
    /// Skill name → version/content-hash for every skill whose content is
    /// currently injected.
    pub skill_versions: serde_json::Value,
    /// Pinned agent composition id: identifies the exact agent definition
    /// (base prompt + overlay + tool allowlist + model tier + skill set)
    /// that produced this request. Makes harness variants A/B-able.
    pub composition_id: String,
}

/// The typed event vocabulary of the ledger.
///
/// Every record that matters — human messages, model requests/outputs, tool
/// calls, approvals, escalations, skill injections, feedback — is the same
/// kind of record (Buzz pattern), which is what makes one queryable audit
/// trail and one training-export substrate possible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum AgentEventPayload {
    /// A turn (user-visible unit of work) started.
    TurnStart {
        /// 0-based turn index within the session.
        turn: i64,
    },
    /// A turn ended, with a typed outcome.
    TurnEnd {
        /// 0-based turn index within the session.
        turn: i64,
        /// Why the turn ended.
        reason: TurnEndReason,
    },
    /// A step (one model request plus its tool calls) started.
    StepStart {
        /// Turn this step belongs to.
        turn: i64,
        /// 0-based step index within the turn.
        step: i64,
    },
    /// A step ended.
    StepEnd {
        /// Turn this step belongs to.
        turn: i64,
        /// 0-based step index within the turn.
        step: i64,
    },
    /// A user-role message entered the session.
    UserMessage {
        /// Message content (text).
        content: String,
        /// Where the message came from.
        source: UserMessageSource,
    },
    /// An assistant message was produced.
    AssistantMessage {
        /// Message content as produced by the model (raw; truncated or
        /// empty messages are logged too, with usage).
        content: String,
        /// Provider that produced it.
        provider: String,
        /// Model that produced it.
        model: String,
        /// Token accounting for the request that produced this message.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
    },
    /// The model called a tool. Arguments are stored raw and unparsed,
    /// exactly as the model produced them — RL needs malformed calls too.
    ToolCall {
        /// Correlation id linking this call to its result.
        call_id: String,
        /// Tool name.
        name: String,
        /// The raw JSON argument string as emitted by the model.
        arguments_raw: String,
    },
    /// A tool returned a result (or a structured error).
    ToolResult {
        /// Correlation id linking back to the call.
        call_id: String,
        /// Tool name.
        name: String,
        /// The model-facing result content.
        content: serde_json::Value,
        /// Structured error, when the tool failed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<ToolError>,
    },
    /// Full request-envelope snapshot (see [`RequestHeader`]).
    RequestHeader(RequestHeader),
    /// The agent asked for human approval before running a gated tool.
    ApprovalRequested {
        /// Approval item id.
        approval_id: Uuid,
        /// The gated tool.
        tool_name: String,
        /// SHA-256 digest of the proposed arguments (raw args are on the
        /// preceding `ToolCall` event).
        arguments_digest: String,
    },
    /// A human decided an approval request.
    ApprovalDecided {
        /// Approval item id.
        approval_id: Uuid,
        /// `true` = approved, `false` = denied.
        approved: bool,
        /// Macro user id of the decider.
        decided_by: String,
        /// Optional note from the decider.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// The agent escalated to a human expert.
    EscalationCreated {
        /// Escalation entity id.
        escalation_id: Uuid,
        /// Domain the escalation was routed to (e.g. `techops`).
        domain: String,
    },
    /// An escalation was resolved and the session resumed.
    EscalationResolved {
        /// Escalation entity id.
        escalation_id: Uuid,
        /// Macro user id of the resolving expert.
        resolved_by: String,
    },
    /// A skill's content was injected into model-visible context. Typed so
    /// per-skill outcome attribution is a ledger query.
    SkillInjected {
        /// Skill identifier.
        skill_id: String,
        /// Skill version or content hash at injection time.
        version: String,
    },
    /// Immutable human feedback captured in-log. Editable ratings live in a
    /// sidecar; the trace itself never changes.
    FeedbackRecord {
        /// Rating: `true` positive, `false` negative, `None` note-only.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rating: Option<bool>,
        /// Free-form note.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        /// The event seq this feedback targets (usually an assistant
        /// message), when targeted.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_seq: Option<i64>,
    },
    /// Context compaction replaced a range of model-visible history with a
    /// summary. Recorded in-log so "what the model actually saw" stays
    /// reconstructable after summarization.
    Compaction {
        /// First replaced seq (inclusive).
        replaced_from_seq: i64,
        /// Last replaced seq (inclusive).
        replaced_to_seq: i64,
        /// The summary text that now stands in for the replaced range.
        summary: String,
    },
    /// Boundary marker separating inherited history (fork/resume) from live
    /// work. Carries fork lineage for rejection-sampling data generation.
    SessionSeed {
        /// Parent session this session was forked/resumed from.
        parent_session_id: Uuid,
        /// Number of inherited events.
        seed_length: i64,
    },
}

/// Structured tool error carried on a [`AgentEventPayload::ToolResult`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolError {
    /// Stable machine-readable error code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

impl AgentEventPayload {
    /// Stable discriminant string used for the indexed `event_type` column.
    pub fn event_type(&self) -> &'static str {
        match self {
            AgentEventPayload::TurnStart { .. } => "turn/start",
            AgentEventPayload::TurnEnd { .. } => "turn/end",
            AgentEventPayload::StepStart { .. } => "step/start",
            AgentEventPayload::StepEnd { .. } => "step/end",
            AgentEventPayload::UserMessage { .. } => "user/message",
            AgentEventPayload::AssistantMessage { .. } => "assistant/message",
            AgentEventPayload::ToolCall { .. } => "tool/call",
            AgentEventPayload::ToolResult { .. } => "tool/result",
            AgentEventPayload::RequestHeader(_) => "request/header",
            AgentEventPayload::ApprovalRequested { .. } => "approval/requested",
            AgentEventPayload::ApprovalDecided { .. } => "approval/decided",
            AgentEventPayload::EscalationCreated { .. } => "escalation/created",
            AgentEventPayload::EscalationResolved { .. } => "escalation/resolved",
            AgentEventPayload::SkillInjected { .. } => "skill/injected",
            AgentEventPayload::FeedbackRecord { .. } => "feedback/record",
            AgentEventPayload::Compaction { .. } => "compaction",
            AgentEventPayload::SessionSeed { .. } => "session/seed",
        }
    }
}

/// A new event submitted for appending; the service assigns `seq` and the
/// hash-chain position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAgentEvent {
    /// The typed payload.
    pub payload: AgentEventPayload,
    /// Who produced the event.
    pub actor: Actor,
    /// When the event occurred (producer clock). Defaults to now upstream
    /// when absent.
    pub occurred_at: DateTime<Utc>,
    /// Provenance: seqs of earlier events that produced this one (e.g. a
    /// derived message's sources).
    #[serde(default)]
    pub source_event_seqs: Vec<i64>,
}

/// A stored, chained ledger event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    /// Session this event belongs to.
    pub session_id: Uuid,
    /// Monotonic, contiguous position within the session (0-based).
    pub seq: i64,
    /// The typed payload.
    pub payload: AgentEventPayload,
    /// Who produced the event.
    pub actor: Actor,
    /// Organization scope, when known.
    pub org_id: Option<i32>,
    /// When the event occurred.
    pub occurred_at: DateTime<Utc>,
    /// Provenance links to earlier events.
    pub source_event_seqs: Vec<i64>,
    /// Hash of the previous event in this session's chain (zeros for the
    /// first event).
    pub prev_hash: Vec<u8>,
    /// This event's chain hash.
    pub hash: Vec<u8>,
}

/// The zero hash used as `prev_hash` for a session's first event.
pub const GENESIS_HASH: [u8; 32] = [0u8; 32];

/// Compute the tamper-evident chain hash for an event.
///
/// `hash = SHA-256(prev_hash || session_id || seq_be || occurred_at_rfc3339
/// || actor_kind || actor_id || payload_json)`
pub fn chain_hash(
    prev_hash: &[u8],
    session_id: Uuid,
    seq: i64,
    occurred_at: DateTime<Utc>,
    actor: &Actor,
    payload_json: &[u8],
) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash);
    hasher.update(session_id.as_bytes());
    hasher.update(seq.to_be_bytes());
    hasher.update(occurred_at.to_rfc3339().as_bytes());
    hasher.update(actor.kind.as_str().as_bytes());
    hasher.update(actor.id.as_bytes());
    hasher.update(payload_json);
    hasher.finalize().to_vec()
}

/// Terminal outcome of a session, derived from `turn/end` and escalation
/// events. Future sessions over similar issues start from these instead of
/// from scratch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionOutcome {
    /// The request was resolved.
    Resolved,
    /// The request was not resolved.
    Unresolved,
    /// The request was escalated to a human.
    Escalated,
}

impl SessionOutcome {
    /// Stable string used for storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionOutcome::Resolved => "resolved",
            SessionOutcome::Unresolved => "unresolved",
            SessionOutcome::Escalated => "escalated",
        }
    }

    /// Parse from the stored string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "resolved" => Some(SessionOutcome::Resolved),
            "unresolved" => Some(SessionOutcome::Unresolved),
            "escalated" => Some(SessionOutcome::Escalated),
            _ => None,
        }
    }
}

/// Kinds of external threads a session can be keyed by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalThreadKind {
    /// A Slack thread (`<channel_id>:<thread_ts>`).
    SlackThread,
    /// An email thread id.
    EmailThread,
    /// A Macro channel thread.
    ChannelThread,
    /// A native Macro AI chat.
    NativeChat,
}

impl ExternalThreadKind {
    /// Stable string used for storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            ExternalThreadKind::SlackThread => "slack_thread",
            ExternalThreadKind::EmailThread => "email_thread",
            ExternalThreadKind::ChannelThread => "channel_thread",
            ExternalThreadKind::NativeChat => "native_chat",
        }
    }

    /// Parse from the stored string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "slack_thread" => Some(ExternalThreadKind::SlackThread),
            "email_thread" => Some(ExternalThreadKind::EmailThread),
            "channel_thread" => Some(ExternalThreadKind::ChannelThread),
            "native_chat" => Some(ExternalThreadKind::NativeChat),
            _ => None,
        }
    }
}

/// Mapping between a runtime conversation, a Macro ledger session, and the
/// external thread that anchors it. Inbox items, escalations, and audit rows
/// deep-link through this mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMapping {
    /// Macro-side session id (primary key of the ledger session).
    pub session_id: Uuid,
    /// The Flue conversation id (or other runtime conversation key).
    pub runtime_conversation_id: String,
    /// The kind of external thread anchoring the session, when any.
    pub external_thread_kind: Option<ExternalThreadKind>,
    /// The external thread key (Slack `channel:thread_ts`, email thread id,
    /// ...), when any.
    pub external_thread_key: Option<String>,
    /// Organization scope, when known.
    pub org_id: Option<i32>,
    /// The agent principal owning the session.
    pub agent_principal_id: Uuid,
    /// When the mapping was created.
    pub created_at: DateTime<Utc>,
}

/// Errors returned by ledger operations.
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    /// The request is invalid.
    #[error("{0}")]
    InvalidRequest(String),
    /// Another writer appended concurrently; the append should be retried.
    #[error("chain conflict at seq {expected_seq}")]
    ChainConflict {
        /// The seq that was expected to be free.
        expected_seq: i64,
    },
    /// The session was not found.
    #[error("session not found")]
    SessionNotFound,
    /// A payload could not be serialized.
    #[error("payload serialization failed")]
    Serialization(#[source] serde_json::Error),
    /// The storage backend failed.
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Result alias for ledger operations.
pub type Result<T> = std::result::Result<T, LedgerError>;
