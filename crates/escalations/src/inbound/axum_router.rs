//! HTTP surface for escalations.
//!
//! Three caller classes:
//! - **Agent principals** (bearer `mat_...` tokens): create escalations and
//!   poll their status. Scope and tenancy policy live in
//!   [`AgentEscalationFacade`].
//! - **Users** (Macro auth): personal and team inbox views, claim,
//!   reassign, resolve, cancel, and transition history.
//! - **Internal callers**: routing-rule and expert-profile administration.

use std::sync::Arc;

use agent_identity::domain::model::IdentityError;
use agent_identity::domain::ports::AgentIdentityService;
use agent_identity::inbound::AgentBearer;
use axum::extract::{FromRef, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use macro_authorization::{
    InternalOnly, MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState,
    UserOrInternal,
};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::facade::AgentEscalationFacade;
use crate::domain::model::{
    Escalation, EscalationError, EscalationTransition, ExpertProfile, NewEscalation, Priority,
    RoutingRule,
};
use crate::domain::ports::RoutingRepo;
use crate::domain::service::{Caller, EscalationService};

/// Router state for the escalation surface.
pub struct EscalationRouterState<E, RT, I, Auth> {
    /// Agent-facing facade (scope + tenancy policy).
    pub facade: Arc<AgentEscalationFacade<E>>,
    /// The escalation service for user-facing operations.
    pub service: Arc<E>,
    /// Routing configuration storage for admin endpoints.
    pub routing: Arc<RT>,
    /// Identity service used to verify agent bearer tokens.
    pub identity: Arc<I>,
    /// Authorization state for user/internal extraction.
    pub authorization_state: MacroAuthorizationState<Auth>,
}

impl<E, RT, I, Auth> Clone for EscalationRouterState<E, RT, I, Auth> {
    fn clone(&self) -> Self {
        Self {
            facade: self.facade.clone(),
            service: self.service.clone(),
            routing: self.routing.clone(),
            identity: self.identity.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<E, RT, I, Auth> FromRef<EscalationRouterState<E, RT, I, Auth>>
    for MacroAuthorizationState<Auth>
{
    fn from_ref(state: &EscalationRouterState<E, RT, I, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Error body for escalation endpoints.
#[derive(Debug, Serialize, ToSchema)]
pub struct EscalationErrorBody {
    /// Error description.
    pub error: String,
}

fn error_response(e: EscalationError) -> Response {
    let status = match &e {
        EscalationError::NotFound => StatusCode::NOT_FOUND,
        EscalationError::InvalidStatus(_) => StatusCode::CONFLICT,
        EscalationError::Forbidden(_) | EscalationError::MissingScope { .. } => {
            StatusCode::FORBIDDEN
        }
        EscalationError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
        EscalationError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    if status == StatusCode::INTERNAL_SERVER_ERROR {
        tracing::error!(error = ?e, "escalation endpoint failed");
        return (
            status,
            Json(EscalationErrorBody {
                error: "internal error".to_string(),
            }),
        )
            .into_response();
    }
    (
        status,
        Json(EscalationErrorBody {
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
        Json(EscalationErrorBody {
            error: "agent authentication failed".to_string(),
        }),
    )
        .into_response()
}

/// Request body for creating an escalation (agent callers).
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateEscalationRequest {
    /// Domain the escalation belongs to (e.g. `techops`).
    pub domain: String,
    /// Ledger session of the escalating conversation.
    pub session_id: Option<Uuid>,
    /// Macro user id of the requester, when known.
    pub requester_user_id: Option<String>,
    /// Display name of the requester (e.g. Slack handle).
    pub requester_display: String,
    /// Source channel (e.g. `slack`).
    pub source_channel: Option<String>,
    /// Short title for inbox cards.
    pub title: String,
    /// What the agent tried and where it got stuck.
    pub summary: String,
    /// Routing tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Urgency; defaults to `normal`.
    pub priority: Option<Priority>,
    /// URL the runtime is called back on when the escalation resolves.
    pub callback_url: Option<String>,
}

/// Personal inbox view.
#[derive(Debug, Serialize, ToSchema)]
pub struct UserEscalationsResponse {
    /// Items assigned to (or claimed by) the user, non-terminal.
    pub assigned: Vec<Escalation>,
    /// Unclaimed items on the user's teams' queues.
    pub claimable: Vec<Escalation>,
}

/// Request body for reassigning an escalation.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ReassignRequest {
    /// New assignee user (exactly one of user/team).
    pub to_user_id: Option<String>,
    /// New assignee team queue (exactly one of user/team).
    pub to_team_id: Option<Uuid>,
    /// Why it was reassigned (required; feeds routing refinement).
    pub reason: String,
}

/// Request body for resolving an escalation.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveRequest {
    /// The expert's answer, delivered back to the agent runtime.
    pub resolution: String,
}

/// Build the escalations router.
pub fn escalations_router<E, RT, I, Auth, S>(
    state: EscalationRouterState<E, RT, I, Auth>,
) -> Router<S>
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
    S: Send + Sync + Clone + 'static,
{
    Router::new()
        .route(
            "/agent-escalations",
            post(agent_create_escalation_handler::<E, RT, I, Auth>),
        )
        .route(
            "/agent-escalations/{id}",
            get(agent_get_escalation_handler::<E, RT, I, Auth>),
        )
        .route(
            "/escalations/mine",
            get(list_my_escalations_handler::<E, RT, I, Auth>),
        )
        .route(
            "/escalations/teams/{team_id}",
            get(list_team_escalations_handler::<E, RT, I, Auth>),
        )
        .route(
            "/escalations/{id}",
            get(get_escalation_handler::<E, RT, I, Auth>),
        )
        .route(
            "/escalations/{id}/claim",
            post(claim_escalation_handler::<E, RT, I, Auth>),
        )
        .route(
            "/escalations/{id}/reassign",
            post(reassign_escalation_handler::<E, RT, I, Auth>),
        )
        .route(
            "/escalations/{id}/resolve",
            post(resolve_escalation_handler::<E, RT, I, Auth>),
        )
        .route(
            "/escalations/{id}/cancel",
            post(cancel_escalation_handler::<E, RT, I, Auth>),
        )
        .route(
            "/escalations/{id}/transitions",
            get(list_transitions_handler::<E, RT, I, Auth>),
        )
        .route(
            "/escalation-routing/rules",
            get(list_rules_handler::<E, RT, I, Auth>).put(upsert_rule_handler::<E, RT, I, Auth>),
        )
        .route(
            "/escalation-routing/rules/{id}",
            axum::routing::delete(delete_rule_handler::<E, RT, I, Auth>),
        )
        .route(
            "/escalation-routing/experts",
            get(list_experts_handler::<E, RT, I, Auth>),
        )
        .route(
            "/escalation-routing/experts",
            put(upsert_expert_handler::<E, RT, I, Auth>),
        )
        .with_state(state)
}

/// Create an escalation. Agent callers.
#[utoipa::path(
    post,
    path = "/agent-escalations",
    request_body = CreateEscalationRequest,
    responses(
        (status = 200, description = "The routed escalation", body = Escalation),
        (status = 400, description = "Invalid request", body = EscalationErrorBody),
        (status = 403, description = "Missing scope", body = EscalationErrorBody),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn agent_create_escalation_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    bearer: AgentBearer,
    Json(body): Json<CreateEscalationRequest>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    let request = NewEscalation {
        domain: body.domain,
        session_id: body.session_id,
        requester_user_id: body.requester_user_id,
        requester_display: body.requester_display,
        source_channel: body.source_channel,
        title: body.title,
        summary: body.summary,
        tags: body.tags,
        priority: body.priority.unwrap_or(Priority::Normal),
        callback_url: body.callback_url,
    };
    match state.facade.create(&agent, request).await {
        Ok(escalation) => Json(escalation).into_response(),
        Err(e) => error_response(e),
    }
}

/// Poll one escalation. Agent callers.
#[utoipa::path(
    get,
    path = "/agent-escalations/{id}",
    params(("id" = Uuid, Path, description = "Escalation id")),
    responses(
        (status = 200, description = "The escalation", body = Escalation),
        (status = 404, description = "Not found", body = EscalationErrorBody),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn agent_get_escalation_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    bearer: AgentBearer,
    Path(id): Path<Uuid>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state.facade.get(&agent, id).await {
        Ok(escalation) => Json(escalation).into_response(),
        Err(e) => error_response(e),
    }
}

/// The caller's personal inbox view: assigned plus claimable items.
#[utoipa::path(
    get,
    path = "/escalations/mine",
    responses(
        (status = 200, description = "Assigned and claimable escalations", body = UserEscalationsResponse),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn list_my_escalations_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let user_id = user.authorization.user.macro_user_id.as_ref().to_string();
    match state.service.list_for_user(&user_id).await {
        Ok(items) => Json(UserEscalationsResponse {
            assigned: items.assigned,
            claimable: items.claimable,
        })
        .into_response(),
        Err(e) => error_response(e),
    }
}

/// A team's full queue (members only): unclaimed and claimed items.
#[utoipa::path(
    get,
    path = "/escalations/teams/{team_id}",
    params(("team_id" = Uuid, Path, description = "Team id")),
    responses(
        (status = 200, description = "The team's escalations", body = [Escalation]),
        (status = 403, description = "Not a member", body = EscalationErrorBody),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn list_team_escalations_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(team_id): Path<Uuid>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state.service.list_for_team(&caller, team_id).await {
        Ok(items) => Json(items).into_response(),
        Err(e) => error_response(e),
    }
}

/// Fetch one escalation visible to the caller.
#[utoipa::path(
    get,
    path = "/escalations/{id}",
    params(("id" = Uuid, Path, description = "Escalation id")),
    responses(
        (status = 200, description = "The escalation", body = Escalation),
        (status = 404, description = "Not found", body = EscalationErrorBody),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn get_escalation_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(id): Path<Uuid>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state.service.get(&caller, id).await {
        Ok(escalation) => Json(escalation).into_response(),
        Err(e) => error_response(e),
    }
}

/// Claim an open escalation (first claim wins).
#[utoipa::path(
    post,
    path = "/escalations/{id}/claim",
    params(("id" = Uuid, Path, description = "Escalation id")),
    responses(
        (status = 200, description = "The claimed escalation", body = Escalation),
        (status = 403, description = "Not allowed", body = EscalationErrorBody),
        (status = 409, description = "Not open", body = EscalationErrorBody),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn claim_escalation_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(id): Path<Uuid>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state.service.claim(&caller, id).await {
        Ok(escalation) => Json(escalation).into_response(),
        Err(e) => error_response(e),
    }
}

/// Reassign to another user or team queue, with a required reason.
#[utoipa::path(
    post,
    path = "/escalations/{id}/reassign",
    params(("id" = Uuid, Path, description = "Escalation id")),
    request_body = ReassignRequest,
    responses(
        (status = 200, description = "The reassigned escalation", body = Escalation),
        (status = 400, description = "Invalid request", body = EscalationErrorBody),
        (status = 403, description = "Not allowed", body = EscalationErrorBody),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn reassign_escalation_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(id): Path<Uuid>,
    Json(body): Json<ReassignRequest>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state
        .service
        .reassign(&caller, id, body.to_user_id, body.to_team_id, body.reason)
        .await
    {
        Ok(escalation) => Json(escalation).into_response(),
        Err(e) => error_response(e),
    }
}

/// Resolve with an expert answer; resumes the agent runtime.
#[utoipa::path(
    post,
    path = "/escalations/{id}/resolve",
    params(("id" = Uuid, Path, description = "Escalation id")),
    request_body = ResolveRequest,
    responses(
        (status = 200, description = "The resolved escalation", body = Escalation),
        (status = 403, description = "Not allowed", body = EscalationErrorBody),
        (status = 409, description = "Already terminal", body = EscalationErrorBody),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn resolve_escalation_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(id): Path<Uuid>,
    Json(body): Json<ResolveRequest>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state.service.resolve(&caller, id, body.resolution).await {
        Ok(escalation) => Json(escalation).into_response(),
        Err(e) => error_response(e),
    }
}

/// Cancel without a resolution.
#[utoipa::path(
    post,
    path = "/escalations/{id}/cancel",
    params(("id" = Uuid, Path, description = "Escalation id")),
    responses(
        (status = 200, description = "The cancelled escalation", body = Escalation),
        (status = 403, description = "Not allowed", body = EscalationErrorBody),
        (status = 409, description = "Already terminal", body = EscalationErrorBody),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn cancel_escalation_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(id): Path<Uuid>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state.service.cancel(&caller, id).await {
        Ok(escalation) => Json(escalation).into_response(),
        Err(e) => error_response(e),
    }
}

/// An escalation's audited transition history.
#[utoipa::path(
    get,
    path = "/escalations/{id}/transitions",
    params(("id" = Uuid, Path, description = "Escalation id")),
    responses(
        (status = 200, description = "Transitions, oldest first", body = [EscalationTransition]),
        (status = 404, description = "Not found", body = EscalationErrorBody),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn list_transitions_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(id): Path<Uuid>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state.service.transitions(&caller, id).await {
        Ok(transitions) => Json(transitions).into_response(),
        Err(e) => error_response(e),
    }
}

/// List routing rules. Internal callers only.
#[utoipa::path(
    get,
    path = "/escalation-routing/rules",
    responses(
        (status = 200, description = "All routing rules", body = [RoutingRule]),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn list_rules_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    match state.routing.list_all_rules(None).await {
        Ok(rules) => Json(rules).into_response(),
        Err(e) => error_response(e),
    }
}

/// Insert or replace a routing rule. Internal callers only.
#[utoipa::path(
    put,
    path = "/escalation-routing/rules",
    request_body = RoutingRule,
    responses(
        (status = 200, description = "The stored rule", body = RoutingRule),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn upsert_rule_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Json(rule): Json<RoutingRule>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    match state.routing.upsert_rule(&rule).await {
        Ok(()) => Json(rule).into_response(),
        Err(e) => error_response(e),
    }
}

/// Delete a routing rule. Internal callers only.
#[utoipa::path(
    delete,
    path = "/escalation-routing/rules/{id}",
    params(("id" = Uuid, Path, description = "Rule id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "No such rule", body = EscalationErrorBody),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn delete_rule_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Path(id): Path<Uuid>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    match state.routing.delete_rule(id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => error_response(EscalationError::NotFound),
        Err(e) => error_response(e),
    }
}

/// List expert profiles. Internal callers only.
#[utoipa::path(
    get,
    path = "/escalation-routing/experts",
    responses(
        (status = 200, description = "All expert profiles", body = [ExpertProfile]),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn list_experts_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    match state.routing.list_experts(None).await {
        Ok(experts) => Json(experts).into_response(),
        Err(e) => error_response(e),
    }
}

/// Insert or replace an expert profile (availability, domains, tags).
/// Internal callers only.
#[utoipa::path(
    put,
    path = "/escalation-routing/experts",
    request_body = ExpertProfile,
    responses(
        (status = 200, description = "The stored profile", body = ExpertProfile),
    ),
    tag = "escalations"
)]
#[tracing::instrument(skip_all)]
pub async fn upsert_expert_handler<E, RT, I, Auth>(
    State(state): State<EscalationRouterState<E, RT, I, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Json(profile): Json<ExpertProfile>,
) -> Response
where
    E: EscalationService,
    RT: RoutingRepo,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    match state.routing.upsert_expert(&profile).await {
        Ok(()) => Json(profile).into_response(),
        Err(e) => error_response(e),
    }
}
