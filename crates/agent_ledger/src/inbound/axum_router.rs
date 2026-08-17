//! HTTP surface for the session ledger.
//!
//! Two caller classes:
//! - **Agent principals** (bearer `mat_...` tokens): open sessions, append
//!   events, replay their own sessions, record outcomes. Scope and tenancy
//!   policy live in [`AgentLedgerFacade`].
//! - **Internal callers**: cross-session audit query, NDJSON export, and
//!   chain verification. User-facing audit UIs go through internal
//!   admin/BFF surfaces.

use std::sync::Arc;

use agent_identity::domain::model::IdentityError;
use agent_identity::domain::ports::AgentIdentityService;
use agent_identity::inbound::AgentBearer;
use axum::extract::{FromRef, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use macro_authorization::{
    InternalOnly, MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState,
};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::facade::{AgentLedgerFacade, OpenSession};
use crate::domain::model::{
    Actor, ActorKind, AgentEvent, AgentEventPayload, ExternalThreadKind, LedgerError,
    NewAgentEvent, SessionMapping, SessionOutcome,
};
use crate::domain::ports::{EventFilter, LedgerService, SessionMappingRepo};

/// Router state for the ledger surface.
pub struct AgentLedgerRouterState<L, M, I, Auth> {
    /// Agent-facing facade (scope + tenancy policy).
    pub facade: Arc<AgentLedgerFacade<L, M>>,
    /// The plain ledger service for internal audit/query/export.
    pub ledger: Arc<L>,
    /// Identity service used to verify agent bearer tokens.
    pub identity: Arc<I>,
    /// Authorization state for internal-caller extraction.
    pub authorization_state: MacroAuthorizationState<Auth>,
}

impl<L, M, I, Auth> Clone for AgentLedgerRouterState<L, M, I, Auth> {
    fn clone(&self) -> Self {
        Self {
            facade: self.facade.clone(),
            ledger: self.ledger.clone(),
            identity: self.identity.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<L, M, I, Auth> FromRef<AgentLedgerRouterState<L, M, I, Auth>>
    for MacroAuthorizationState<Auth>
{
    fn from_ref(state: &AgentLedgerRouterState<L, M, I, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Error body for ledger endpoints.
#[derive(Debug, Serialize, ToSchema)]
pub struct LedgerErrorBody {
    /// Error description.
    pub error: String,
}

fn ledger_error_response(e: LedgerError) -> Response {
    let status = match &e {
        LedgerError::InvalidRequest(_) | LedgerError::Serialization(_) => StatusCode::BAD_REQUEST,
        LedgerError::SessionNotFound => StatusCode::NOT_FOUND,
        LedgerError::MissingScope { .. } => StatusCode::FORBIDDEN,
        LedgerError::ChainConflict { .. } => StatusCode::CONFLICT,
        LedgerError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    if status == StatusCode::INTERNAL_SERVER_ERROR {
        tracing::error!(error = ?e, "ledger endpoint failed");
        return (
            status,
            Json(LedgerErrorBody {
                error: "internal error".to_string(),
            }),
        )
            .into_response();
    }
    (
        status,
        Json(LedgerErrorBody {
            error: e.to_string(),
        }),
    )
        .into_response()
}

fn identity_error_response(e: IdentityError) -> Response {
    let status = match &e {
        IdentityError::MalformedToken => StatusCode::BAD_REQUEST,
        IdentityError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::UNAUTHORIZED,
    };
    (
        status,
        Json(LedgerErrorBody {
            error: "agent authentication failed".to_string(),
        }),
    )
        .into_response()
}

/// Request body for opening a session.
#[derive(Debug, Deserialize, ToSchema)]
pub struct OpenSessionRequest {
    /// The runtime (Flue) conversation id.
    pub runtime_conversation_id: String,
    /// `slack_thread` | `email_thread` | `channel_thread` | `native_chat`
    pub external_thread_kind: Option<String>,
    /// The external thread key.
    pub external_thread_key: Option<String>,
}

/// Response body describing a session mapping.
#[derive(Debug, Serialize, ToSchema)]
pub struct SessionMappingResponse {
    /// Macro session id.
    pub session_id: Uuid,
    /// Runtime conversation id.
    pub runtime_conversation_id: String,
    /// External thread kind, when anchored.
    pub external_thread_kind: Option<String>,
    /// External thread key, when anchored.
    pub external_thread_key: Option<String>,
    /// Organization scope.
    pub org_id: Option<i32>,
    /// Owning agent principal.
    pub agent_principal_id: Uuid,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

impl From<SessionMapping> for SessionMappingResponse {
    fn from(m: SessionMapping) -> Self {
        Self {
            session_id: m.session_id,
            runtime_conversation_id: m.runtime_conversation_id,
            external_thread_kind: m.external_thread_kind.map(|k| k.as_str().to_string()),
            external_thread_key: m.external_thread_key,
            org_id: m.org_id,
            agent_principal_id: m.agent_principal_id,
            created_at: m.created_at,
        }
    }
}

/// A new event submitted for appending.
#[derive(Debug, Deserialize, ToSchema)]
pub struct NewEventBody {
    /// The typed payload, adjacently tagged (`{"type": ..., "data": ...}`).
    #[schema(value_type = Object)]
    pub payload: AgentEventPayload,
    /// `user` | `agent` | `system`
    pub actor_kind: String,
    /// Actor identifier.
    pub actor_id: String,
    /// When the event occurred; defaults to now.
    pub occurred_at: Option<DateTime<Utc>>,
    /// Provenance links to earlier events in this session.
    #[serde(default)]
    pub source_event_seqs: Vec<i64>,
}

/// Request body for appending events.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AppendEventsRequest {
    /// The events to append, in order.
    pub events: Vec<NewEventBody>,
}

/// A stored ledger event.
#[derive(Debug, Serialize, ToSchema)]
pub struct EventResponse {
    /// Session id.
    pub session_id: Uuid,
    /// Position in the session.
    pub seq: i64,
    /// Stored event type discriminant.
    pub event_type: String,
    /// The typed payload, adjacently tagged.
    #[schema(value_type = Object)]
    pub payload: AgentEventPayload,
    /// Actor kind.
    pub actor_kind: String,
    /// Actor id.
    pub actor_id: String,
    /// Organization scope.
    pub org_id: Option<i32>,
    /// When the event occurred.
    pub occurred_at: DateTime<Utc>,
    /// Provenance links.
    pub source_event_seqs: Vec<i64>,
    /// Chain hash (hex).
    pub hash: String,
}

impl From<AgentEvent> for EventResponse {
    fn from(e: AgentEvent) -> Self {
        Self {
            session_id: e.session_id,
            seq: e.seq,
            event_type: e.payload.event_type().to_string(),
            payload: e.payload,
            actor_kind: e.actor.kind.as_str().to_string(),
            actor_id: e.actor.id,
            org_id: e.org_id,
            occurred_at: e.occurred_at,
            source_event_seqs: e.source_event_seqs,
            hash: hex::encode(e.hash),
        }
    }
}

fn parse_new_events(events: Vec<NewEventBody>) -> Result<Vec<NewAgentEvent>, LedgerError> {
    events
        .into_iter()
        .map(|e| {
            let kind = ActorKind::parse(&e.actor_kind).ok_or_else(|| {
                LedgerError::InvalidRequest(format!("unknown actor kind: {}", e.actor_kind))
            })?;
            Ok(NewAgentEvent {
                payload: e.payload,
                actor: Actor {
                    kind,
                    id: e.actor_id,
                },
                occurred_at: e.occurred_at.unwrap_or_else(Utc::now),
                source_event_seqs: e.source_event_seqs,
            })
        })
        .collect()
}

/// Build the ledger router.
pub fn agent_ledger_router<L, M, I, Auth, S>(
    state: AgentLedgerRouterState<L, M, I, Auth>,
) -> Router<S>
where
    L: LedgerService,
    M: SessionMappingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
    S: Send + Sync + Clone + 'static,
{
    Router::new()
        .route(
            "/agent-ledger/sessions",
            post(open_session_handler::<L, M, I, Auth>),
        )
        .route(
            "/agent-ledger/sessions/by-thread",
            get(find_session_by_thread_handler::<L, M, I, Auth>),
        )
        .route(
            "/agent-ledger/sessions/{session_id}/events",
            post(append_events_handler::<L, M, I, Auth>)
                .get(list_session_events_handler::<L, M, I, Auth>),
        )
        .route(
            "/agent-ledger/sessions/{session_id}/outcome",
            post(record_outcome_handler::<L, M, I, Auth>),
        )
        .route(
            "/agent-ledger/sessions/{session_id}/verify",
            get(verify_chain_handler::<L, M, I, Auth>),
        )
        .route(
            "/agent-ledger/events",
            get(query_events_handler::<L, M, I, Auth>),
        )
        .route(
            "/agent-ledger/agent-events",
            get(agent_query_events_handler::<L, M, I, Auth>),
        )
        .route(
            "/agent-ledger/export",
            get(export_events_handler::<L, M, I, Auth>),
        )
        .with_state(state)
}

/// Open (or resume) a session for a runtime conversation. Agent callers.
#[utoipa::path(
    post,
    path = "/agent-ledger/sessions",
    request_body = OpenSessionRequest,
    responses(
        (status = 200, description = "The session mapping", body = SessionMappingResponse),
        (status = 400, description = "Invalid request", body = LedgerErrorBody),
        (status = 403, description = "Missing scope", body = LedgerErrorBody),
    ),
    tag = "agent-ledger"
)]
#[tracing::instrument(skip_all)]
pub async fn open_session_handler<L, M, I, Auth>(
    State(state): State<AgentLedgerRouterState<L, M, I, Auth>>,
    bearer: AgentBearer,
    Json(body): Json<OpenSessionRequest>,
) -> Response
where
    L: LedgerService,
    M: SessionMappingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    let external_thread_kind = match body.external_thread_kind.as_deref() {
        None => None,
        Some(s) => match ExternalThreadKind::parse(s) {
            Some(k) => Some(k),
            None => {
                return ledger_error_response(LedgerError::InvalidRequest(format!(
                    "unknown external thread kind: {s}"
                )));
            }
        },
    };
    match state
        .facade
        .open_session(
            &agent,
            OpenSession {
                runtime_conversation_id: body.runtime_conversation_id,
                external_thread_kind,
                external_thread_key: body.external_thread_key,
            },
        )
        .await
    {
        Ok(mapping) => Json(SessionMappingResponse::from(mapping)).into_response(),
        Err(e) => ledger_error_response(e),
    }
}

/// Query parameters for looking up a session by external thread.
#[derive(Debug, Deserialize)]
pub struct ByThreadQuery {
    /// External thread kind.
    pub kind: String,
    /// External thread key.
    pub key: String,
}

/// Find the session anchored to an external thread. Agent callers.
#[utoipa::path(
    get,
    path = "/agent-ledger/sessions/by-thread",
    responses(
        (status = 200, description = "The session mapping", body = SessionMappingResponse),
        (status = 404, description = "No session for this thread", body = LedgerErrorBody),
    ),
    tag = "agent-ledger"
)]
#[tracing::instrument(skip_all)]
pub async fn find_session_by_thread_handler<L, M, I, Auth>(
    State(state): State<AgentLedgerRouterState<L, M, I, Auth>>,
    bearer: AgentBearer,
    Query(query): Query<ByThreadQuery>,
) -> Response
where
    L: LedgerService,
    M: SessionMappingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    let Some(kind) = ExternalThreadKind::parse(&query.kind) else {
        return ledger_error_response(LedgerError::InvalidRequest(format!(
            "unknown external thread kind: {}",
            query.kind
        )));
    };
    match state
        .facade
        .find_session_by_thread(&agent, kind, &query.key)
        .await
    {
        Ok(Some(mapping)) => Json(SessionMappingResponse::from(mapping)).into_response(),
        Ok(None) => ledger_error_response(LedgerError::SessionNotFound),
        Err(e) => ledger_error_response(e),
    }
}

/// Append events to a session. Agent callers.
#[utoipa::path(
    post,
    path = "/agent-ledger/sessions/{session_id}/events",
    request_body = AppendEventsRequest,
    responses(
        (status = 200, description = "The stored events", body = Vec<EventResponse>),
        (status = 400, description = "Invalid request", body = LedgerErrorBody),
        (status = 403, description = "Missing scope", body = LedgerErrorBody),
        (status = 404, description = "Session not found", body = LedgerErrorBody),
    ),
    tag = "agent-ledger"
)]
#[tracing::instrument(skip_all, fields(session_id = %session_id))]
pub async fn append_events_handler<L, M, I, Auth>(
    State(state): State<AgentLedgerRouterState<L, M, I, Auth>>,
    bearer: AgentBearer,
    Path(session_id): Path<Uuid>,
    Json(body): Json<AppendEventsRequest>,
) -> Response
where
    L: LedgerService,
    M: SessionMappingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    let events = match parse_new_events(body.events) {
        Ok(events) => events,
        Err(e) => return ledger_error_response(e),
    };
    match state.facade.append_events(&agent, session_id, events).await {
        Ok(stored) => Json(
            stored
                .into_iter()
                .map(EventResponse::from)
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => ledger_error_response(e),
    }
}

/// Query parameters for replaying a session.
#[derive(Debug, Deserialize)]
pub struct ListEventsQuery {
    /// First seq to return (default 0).
    pub from_seq: Option<i64>,
    /// Maximum events to return.
    pub limit: Option<i64>,
}

/// Replay a session's events in order. Agent callers.
#[utoipa::path(
    get,
    path = "/agent-ledger/sessions/{session_id}/events",
    responses(
        (status = 200, description = "Events in seq order", body = Vec<EventResponse>),
        (status = 404, description = "Session not found", body = LedgerErrorBody),
    ),
    tag = "agent-ledger"
)]
#[tracing::instrument(skip_all, fields(session_id = %session_id))]
pub async fn list_session_events_handler<L, M, I, Auth>(
    State(state): State<AgentLedgerRouterState<L, M, I, Auth>>,
    bearer: AgentBearer,
    Path(session_id): Path<Uuid>,
    Query(query): Query<ListEventsQuery>,
) -> Response
where
    L: LedgerService,
    M: SessionMappingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state
        .facade
        .list_session_events(
            &agent,
            session_id,
            query.from_seq.unwrap_or(0),
            query
                .limit
                .unwrap_or(crate::domain::service::DEFAULT_QUERY_LIMIT),
        )
        .await
    {
        Ok(events) => Json(
            events
                .into_iter()
                .map(EventResponse::from)
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => ledger_error_response(e),
    }
}

/// Request body for recording a session outcome.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RecordOutcomeRequest {
    /// `resolved` | `unresolved` | `escalated`
    pub outcome: String,
    /// Optional summary of what was attempted / what worked.
    pub summary: Option<String>,
}

/// Record a session's terminal outcome. Agent callers.
#[utoipa::path(
    post,
    path = "/agent-ledger/sessions/{session_id}/outcome",
    request_body = RecordOutcomeRequest,
    responses(
        (status = 204, description = "Outcome recorded"),
        (status = 400, description = "Invalid request", body = LedgerErrorBody),
        (status = 404, description = "Session not found", body = LedgerErrorBody),
    ),
    tag = "agent-ledger"
)]
#[tracing::instrument(skip_all, fields(session_id = %session_id))]
pub async fn record_outcome_handler<L, M, I, Auth>(
    State(state): State<AgentLedgerRouterState<L, M, I, Auth>>,
    bearer: AgentBearer,
    Path(session_id): Path<Uuid>,
    Json(body): Json<RecordOutcomeRequest>,
) -> Response
where
    L: LedgerService,
    M: SessionMappingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    let Some(outcome) = SessionOutcome::parse(&body.outcome) else {
        return ledger_error_response(LedgerError::InvalidRequest(format!(
            "unknown outcome: {}",
            body.outcome
        )));
    };
    match state
        .facade
        .record_outcome(&agent, session_id, outcome, body.summary)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => ledger_error_response(e),
    }
}

/// Chain verification result.
#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyChainResponse {
    /// Whether the whole chain verifies.
    pub valid: bool,
    /// The first broken seq when invalid.
    pub first_broken_seq: Option<i64>,
}

/// Verify a session's hash chain. Internal callers.
#[utoipa::path(
    get,
    path = "/agent-ledger/sessions/{session_id}/verify",
    responses(
        (status = 200, description = "Verification result", body = VerifyChainResponse),
    ),
    tag = "agent-ledger"
)]
#[tracing::instrument(skip_all, fields(session_id = %session_id))]
pub async fn verify_chain_handler<L, M, I, Auth>(
    State(state): State<AgentLedgerRouterState<L, M, I, Auth>>,
    _caller: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Path(session_id): Path<Uuid>,
) -> Response
where
    L: LedgerService,
    M: SessionMappingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    match state.ledger.verify_chain(session_id).await {
        Ok(first_broken_seq) => Json(VerifyChainResponse {
            valid: first_broken_seq.is_none(),
            first_broken_seq,
        })
        .into_response(),
        Err(e) => ledger_error_response(e),
    }
}

/// Query parameters for the cross-session audit query.
#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    /// Restrict to a session.
    pub session_id: Option<Uuid>,
    /// Restrict to an organization.
    pub org_id: Option<i32>,
    /// Comma-separated event types (stored discriminants).
    pub event_types: Option<String>,
    /// Restrict to an actor id.
    pub actor_id: Option<String>,
    /// Events at or after this instant.
    pub after: Option<DateTime<Utc>>,
    /// Events before this instant.
    pub before: Option<DateTime<Utc>>,
    /// Maximum rows.
    pub limit: Option<i64>,
}

impl AuditQuery {
    fn into_filter(self) -> EventFilter {
        EventFilter {
            session_id: self.session_id,
            org_id: self.org_id,
            event_types: self
                .event_types
                .map(|s| {
                    s.split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            actor_id: self.actor_id,
            occurred_after: self.after,
            occurred_before: self.before,
            limit: self.limit.unwrap_or(0),
        }
    }
}

/// Cross-session audit query. Internal callers.
#[utoipa::path(
    get,
    path = "/agent-ledger/events",
    responses(
        (status = 200, description = "Matching events", body = Vec<EventResponse>),
    ),
    tag = "agent-ledger"
)]
#[tracing::instrument(skip_all)]
pub async fn query_events_handler<L, M, I, Auth>(
    State(state): State<AgentLedgerRouterState<L, M, I, Auth>>,
    _caller: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Query(query): Query<AuditQuery>,
) -> Response
where
    L: LedgerService,
    M: SessionMappingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    match state.ledger.query_events(query.into_filter()).await {
        Ok(events) => Json(
            events
                .into_iter()
                .map(EventResponse::from)
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => ledger_error_response(e),
    }
}

/// Org-scoped audit query for agent callers (self-query over past attempts,
/// approvals, escalations). The org filter is forced server-side.
#[utoipa::path(
    get,
    path = "/agent-ledger/agent-events",
    responses(
        (status = 200, description = "Matching events in the agent's org", body = Vec<EventResponse>),
        (status = 403, description = "Missing scope", body = LedgerErrorBody),
    ),
    tag = "agent-ledger"
)]
#[tracing::instrument(skip_all)]
pub async fn agent_query_events_handler<L, M, I, Auth>(
    State(state): State<AgentLedgerRouterState<L, M, I, Auth>>,
    bearer: AgentBearer,
    Query(query): Query<AuditQuery>,
) -> Response
where
    L: LedgerService,
    M: SessionMappingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state
        .facade
        .query_org_events(&agent, query.into_filter())
        .await
    {
        Ok(events) => Json(
            events
                .into_iter()
                .map(EventResponse::from)
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => ledger_error_response(e),
    }
}

/// Audit export as NDJSON (one event per line). Internal callers.
#[utoipa::path(
    get,
    path = "/agent-ledger/export",
    responses(
        (status = 200, description = "NDJSON export of matching events"),
    ),
    tag = "agent-ledger"
)]
#[tracing::instrument(skip_all)]
pub async fn export_events_handler<L, M, I, Auth>(
    State(state): State<AgentLedgerRouterState<L, M, I, Auth>>,
    _caller: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Query(query): Query<AuditQuery>,
) -> Response
where
    L: LedgerService,
    M: SessionMappingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    match state.ledger.query_events(query.into_filter()).await {
        Ok(events) => {
            let mut body = String::new();
            for event in events {
                match serde_json::to_string(&EventResponse::from(event)) {
                    Ok(line) => {
                        body.push_str(&line);
                        body.push('\n');
                    }
                    Err(e) => {
                        return ledger_error_response(LedgerError::Serialization(e));
                    }
                }
            }
            (
                StatusCode::OK,
                [(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-ndjson".to_string(),
                )],
                body,
            )
                .into_response()
        }
        Err(e) => ledger_error_response(e),
    }
}
