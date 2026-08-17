//! HTTP surface for approval gates.
//!
//! Three caller classes:
//! - **Agent principals** (bearer `mat_...` tokens): gate proposed tool
//!   calls, poll decisions, cancel their own pending requests. Scope and
//!   tenancy policy live in [`AgentApprovalFacade`].
//! - **Users** (Macro auth): personal and team inbox views, decide,
//!   reassign, and transition history.
//! - **Internal callers**: tool-policy administration.

use std::sync::Arc;

use agent_identity::domain::model::IdentityError;
use agent_identity::domain::ports::AgentIdentityService;
use agent_identity::inbound::AgentBearer;
use axum::extract::{FromRef, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use macro_authorization::{
    InternalOnly, MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState,
    UserOrInternal,
};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::facade::AgentApprovalFacade;
use crate::domain::model::{
    ApprovalError, ApprovalRequest, ApprovalTransition, GateOutcome, GateRequest, ToolPolicy,
};
use crate::domain::ports::PolicyRepo;
use crate::domain::service::{ApprovalService, Caller};

/// Router state for the approval surface.
pub struct ApprovalRouterState<A, P, I, Auth> {
    /// Agent-facing facade (scope + tenancy policy).
    pub facade: Arc<AgentApprovalFacade<A>>,
    /// The approval service for user-facing operations.
    pub service: Arc<A>,
    /// Policy storage for admin endpoints.
    pub policies: Arc<P>,
    /// Identity service used to verify agent bearer tokens.
    pub identity: Arc<I>,
    /// Authorization state for user/internal extraction.
    pub authorization_state: MacroAuthorizationState<Auth>,
}

impl<A, P, I, Auth> Clone for ApprovalRouterState<A, P, I, Auth> {
    fn clone(&self) -> Self {
        Self {
            facade: self.facade.clone(),
            service: self.service.clone(),
            policies: self.policies.clone(),
            identity: self.identity.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<A, P, I, Auth> FromRef<ApprovalRouterState<A, P, I, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &ApprovalRouterState<A, P, I, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Error body for approval endpoints.
#[derive(Debug, Serialize, ToSchema)]
pub struct ApprovalErrorBody {
    /// Error description.
    pub error: String,
}

fn error_response(e: ApprovalError) -> Response {
    let status = match &e {
        ApprovalError::NotFound => StatusCode::NOT_FOUND,
        ApprovalError::InvalidStatus(_) => StatusCode::CONFLICT,
        ApprovalError::Forbidden(_) | ApprovalError::MissingScope { .. } => StatusCode::FORBIDDEN,
        ApprovalError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
        ApprovalError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    if status == StatusCode::INTERNAL_SERVER_ERROR {
        tracing::error!(error = ?e, "approval endpoint failed");
        return (
            status,
            Json(ApprovalErrorBody {
                error: "internal error".to_string(),
            }),
        )
            .into_response();
    }
    (
        status,
        Json(ApprovalErrorBody {
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
        Json(ApprovalErrorBody {
            error: "agent authentication failed".to_string(),
        }),
    )
        .into_response()
}

/// Request body for gating a proposed tool call (agent callers).
#[derive(Debug, Deserialize, ToSchema)]
pub struct GateToolCallRequest {
    /// Ledger session of the conversation.
    pub session_id: Option<Uuid>,
    /// Macro user id of the person the agent acts for, when known.
    pub requester_user_id: Option<String>,
    /// Display name for inbox cards.
    pub requester_display: String,
    /// The tool about to run.
    pub tool_name: String,
    /// The proposed arguments, shown verbatim to the approver.
    pub arguments: serde_json::Value,
    /// The agent's explanation of what it wants to do and why.
    pub summary: String,
    /// URL the runtime is called back on when decided.
    pub callback_url: Option<String>,
}

/// Personal inbox view.
#[derive(Debug, Serialize, ToSchema)]
pub struct UserApprovalsResponse {
    /// Pending requests assigned directly to the user.
    pub assigned: Vec<ApprovalRequest>,
    /// Pending requests on the user's teams' queues.
    pub team_queue: Vec<ApprovalRequest>,
}

/// Request body for deciding a pending approval.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DecideRequest {
    /// `true` approves, `false` denies.
    pub approved: bool,
    /// Optional note shown to the agent and requester.
    pub note: Option<String>,
}

/// Request body for reassigning a pending approval.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ReassignApprovalRequest {
    /// New assignee user (exactly one of user/team).
    pub to_user_id: Option<String>,
    /// New assignee team queue (exactly one of user/team).
    pub to_team_id: Option<Uuid>,
    /// Why it was reassigned (required; recorded in the trail).
    pub reason: String,
}

/// Build the approvals router.
pub fn approvals_router<A, P, I, Auth, S>(state: ApprovalRouterState<A, P, I, Auth>) -> Router<S>
where
    A: ApprovalService,
    P: PolicyRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
    S: Send + Sync + Clone + 'static,
{
    Router::new()
        .route(
            "/agent-approvals/gate",
            post(agent_gate_handler::<A, P, I, Auth>),
        )
        .route(
            "/agent-approvals/{id}",
            get(agent_get_approval_handler::<A, P, I, Auth>),
        )
        .route(
            "/agent-approvals/{id}/cancel",
            post(agent_cancel_approval_handler::<A, P, I, Auth>),
        )
        .route(
            "/approvals/mine",
            get(list_my_approvals_handler::<A, P, I, Auth>),
        )
        .route(
            "/approvals/teams/{team_id}",
            get(list_team_approvals_handler::<A, P, I, Auth>),
        )
        .route(
            "/approvals/{id}",
            get(get_approval_handler::<A, P, I, Auth>),
        )
        .route(
            "/approvals/{id}/decide",
            post(decide_approval_handler::<A, P, I, Auth>),
        )
        .route(
            "/approvals/{id}/reassign",
            post(reassign_approval_handler::<A, P, I, Auth>),
        )
        .route(
            "/approvals/{id}/transitions",
            get(list_approval_transitions_handler::<A, P, I, Auth>),
        )
        .route(
            "/approval-policies",
            get(list_policies_handler::<A, P, I, Auth>).put(upsert_policy_handler::<A, P, I, Auth>),
        )
        .route(
            "/approval-policies/{id}",
            axum::routing::delete(delete_policy_handler::<A, P, I, Auth>),
        )
        .with_state(state)
}

/// Gate a proposed tool call. Agent callers.
#[utoipa::path(
    post,
    path = "/agent-approvals/gate",
    request_body = GateToolCallRequest,
    responses(
        (status = 200, description = "The gate outcome", body = GateOutcome),
        (status = 400, description = "Invalid request", body = ApprovalErrorBody),
        (status = 403, description = "Missing scope", body = ApprovalErrorBody),
    ),
    tag = "approvals"
)]
#[tracing::instrument(skip_all)]
pub async fn agent_gate_handler<A, P, I, Auth>(
    State(state): State<ApprovalRouterState<A, P, I, Auth>>,
    bearer: AgentBearer,
    Json(body): Json<GateToolCallRequest>,
) -> Response
where
    A: ApprovalService,
    P: PolicyRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    let request = GateRequest {
        // Overwritten by the facade with the principal's slug.
        agent_slug: String::new(),
        session_id: body.session_id,
        requester_user_id: body.requester_user_id,
        requester_display: body.requester_display,
        tool_name: body.tool_name,
        arguments: body.arguments,
        summary: body.summary,
        callback_url: body.callback_url,
    };
    match state.facade.gate(&agent, request).await {
        Ok(outcome) => Json(outcome).into_response(),
        Err(e) => error_response(e),
    }
}

/// Poll one approval request. Agent callers.
#[utoipa::path(
    get,
    path = "/agent-approvals/{id}",
    params(("id" = Uuid, Path, description = "Approval request id")),
    responses(
        (status = 200, description = "The approval request", body = ApprovalRequest),
        (status = 404, description = "Not found", body = ApprovalErrorBody),
    ),
    tag = "approvals"
)]
#[tracing::instrument(skip_all)]
pub async fn agent_get_approval_handler<A, P, I, Auth>(
    State(state): State<ApprovalRouterState<A, P, I, Auth>>,
    bearer: AgentBearer,
    Path(id): Path<Uuid>,
) -> Response
where
    A: ApprovalService,
    P: PolicyRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state.facade.get(&agent, id).await {
        Ok(request) => Json(request).into_response(),
        Err(e) => error_response(e),
    }
}

/// Cancel one of the agent's own pending requests. Agent callers.
#[utoipa::path(
    post,
    path = "/agent-approvals/{id}/cancel",
    params(("id" = Uuid, Path, description = "Approval request id")),
    responses(
        (status = 200, description = "The cancelled request", body = ApprovalRequest),
        (status = 404, description = "Not found", body = ApprovalErrorBody),
        (status = 409, description = "Not pending", body = ApprovalErrorBody),
    ),
    tag = "approvals"
)]
#[tracing::instrument(skip_all)]
pub async fn agent_cancel_approval_handler<A, P, I, Auth>(
    State(state): State<ApprovalRouterState<A, P, I, Auth>>,
    bearer: AgentBearer,
    Path(id): Path<Uuid>,
) -> Response
where
    A: ApprovalService,
    P: PolicyRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state.facade.cancel(&agent, id).await {
        Ok(request) => Json(request).into_response(),
        Err(e) => error_response(e),
    }
}

/// The caller's personal inbox view: assigned plus team-queue items.
#[utoipa::path(
    get,
    path = "/approvals/mine",
    responses(
        (status = 200, description = "Assigned and team-queue approvals", body = UserApprovalsResponse),
    ),
    tag = "approvals"
)]
#[tracing::instrument(skip_all)]
pub async fn list_my_approvals_handler<A, P, I, Auth>(
    State(state): State<ApprovalRouterState<A, P, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
) -> Response
where
    A: ApprovalService,
    P: PolicyRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let user_id = user.authorization.user.macro_user_id.as_ref().to_string();
    match state.service.list_for_user(&user_id).await {
        Ok(items) => Json(UserApprovalsResponse {
            assigned: items.assigned,
            team_queue: items.team_queue,
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

/// A team's full queue (members only).
#[utoipa::path(
    get,
    path = "/approvals/teams/{team_id}",
    params(("team_id" = Uuid, Path, description = "Team id")),
    responses(
        (status = 200, description = "The team's approvals", body = [ApprovalRequest]),
        (status = 403, description = "Not a member", body = ApprovalErrorBody),
    ),
    tag = "approvals"
)]
#[tracing::instrument(skip_all)]
pub async fn list_team_approvals_handler<A, P, I, Auth>(
    State(state): State<ApprovalRouterState<A, P, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(team_id): Path<Uuid>,
) -> Response
where
    A: ApprovalService,
    P: PolicyRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state.service.list_for_team(&caller, team_id).await {
        Ok(items) => Json(items).into_response(),
        Err(e) => error_response(e),
    }
}

/// Fetch one approval request visible to the caller.
#[utoipa::path(
    get,
    path = "/approvals/{id}",
    params(("id" = Uuid, Path, description = "Approval request id")),
    responses(
        (status = 200, description = "The approval request", body = ApprovalRequest),
        (status = 404, description = "Not found", body = ApprovalErrorBody),
    ),
    tag = "approvals"
)]
#[tracing::instrument(skip_all)]
pub async fn get_approval_handler<A, P, I, Auth>(
    State(state): State<ApprovalRouterState<A, P, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(id): Path<Uuid>,
) -> Response
where
    A: ApprovalService,
    P: PolicyRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state.service.get(&caller, id).await {
        Ok(request) => Json(request).into_response(),
        Err(e) => error_response(e),
    }
}

/// Approve or deny a pending request; resumes the agent runtime.
#[utoipa::path(
    post,
    path = "/approvals/{id}/decide",
    params(("id" = Uuid, Path, description = "Approval request id")),
    request_body = DecideRequest,
    responses(
        (status = 200, description = "The decided request", body = ApprovalRequest),
        (status = 403, description = "Not allowed", body = ApprovalErrorBody),
        (status = 409, description = "Not pending", body = ApprovalErrorBody),
    ),
    tag = "approvals"
)]
#[tracing::instrument(skip_all)]
pub async fn decide_approval_handler<A, P, I, Auth>(
    State(state): State<ApprovalRouterState<A, P, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(id): Path<Uuid>,
    Json(body): Json<DecideRequest>,
) -> Response
where
    A: ApprovalService,
    P: PolicyRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state
        .service
        .decide(&caller, id, body.approved, body.note)
        .await
    {
        Ok(request) => Json(request).into_response(),
        Err(e) => error_response(e),
    }
}

/// Reassign a pending request to another user or team ("this isn't mine").
#[utoipa::path(
    post,
    path = "/approvals/{id}/reassign",
    params(("id" = Uuid, Path, description = "Approval request id")),
    request_body = ReassignApprovalRequest,
    responses(
        (status = 200, description = "The reassigned request", body = ApprovalRequest),
        (status = 400, description = "Invalid request", body = ApprovalErrorBody),
        (status = 403, description = "Not allowed", body = ApprovalErrorBody),
    ),
    tag = "approvals"
)]
#[tracing::instrument(skip_all)]
pub async fn reassign_approval_handler<A, P, I, Auth>(
    State(state): State<ApprovalRouterState<A, P, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(id): Path<Uuid>,
    Json(body): Json<ReassignApprovalRequest>,
) -> Response
where
    A: ApprovalService,
    P: PolicyRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state
        .service
        .reassign(&caller, id, body.to_user_id, body.to_team_id, body.reason)
        .await
    {
        Ok(request) => Json(request).into_response(),
        Err(e) => error_response(e),
    }
}

/// An approval request's audited transition history.
#[utoipa::path(
    get,
    path = "/approvals/{id}/transitions",
    params(("id" = Uuid, Path, description = "Approval request id")),
    responses(
        (status = 200, description = "Transitions, oldest first", body = [ApprovalTransition]),
        (status = 404, description = "Not found", body = ApprovalErrorBody),
    ),
    tag = "approvals"
)]
#[tracing::instrument(skip_all)]
pub async fn list_approval_transitions_handler<A, P, I, Auth>(
    State(state): State<ApprovalRouterState<A, P, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(id): Path<Uuid>,
) -> Response
where
    A: ApprovalService,
    P: PolicyRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state.service.transitions(&caller, id).await {
        Ok(transitions) => Json(transitions).into_response(),
        Err(e) => error_response(e),
    }
}

/// List tool policies. Internal callers only.
#[utoipa::path(
    get,
    path = "/approval-policies",
    responses(
        (status = 200, description = "All tool policies", body = [ToolPolicy]),
    ),
    tag = "approvals"
)]
#[tracing::instrument(skip_all)]
pub async fn list_policies_handler<A, P, I, Auth>(
    State(state): State<ApprovalRouterState<A, P, I, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
) -> Response
where
    A: ApprovalService,
    P: PolicyRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    match state.policies.list_policies(None).await {
        Ok(policies) => Json(policies).into_response(),
        Err(e) => error_response(e),
    }
}

/// Insert or replace a tool policy. Internal callers only.
#[utoipa::path(
    put,
    path = "/approval-policies",
    request_body = ToolPolicy,
    responses(
        (status = 200, description = "The stored policy", body = ToolPolicy),
    ),
    tag = "approvals"
)]
#[tracing::instrument(skip_all)]
pub async fn upsert_policy_handler<A, P, I, Auth>(
    State(state): State<ApprovalRouterState<A, P, I, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Json(policy): Json<ToolPolicy>,
) -> Response
where
    A: ApprovalService,
    P: PolicyRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    match state.policies.upsert_policy(&policy).await {
        Ok(()) => Json(policy).into_response(),
        Err(e) => error_response(e),
    }
}

/// Delete a tool policy. Internal callers only.
#[utoipa::path(
    delete,
    path = "/approval-policies/{id}",
    params(("id" = Uuid, Path, description = "Policy id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "No such policy", body = ApprovalErrorBody),
    ),
    tag = "approvals"
)]
#[tracing::instrument(skip_all)]
pub async fn delete_policy_handler<A, P, I, Auth>(
    State(state): State<ApprovalRouterState<A, P, I, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Path(id): Path<Uuid>,
) -> Response
where
    A: ApprovalService,
    P: PolicyRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    match state.policies.delete_policy(id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => error_response(ApprovalError::NotFound),
        Err(e) => error_response(e),
    }
}
