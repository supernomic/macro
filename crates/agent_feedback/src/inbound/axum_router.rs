//! HTTP surface for the feedback sidecar.
//!
//! Agent principals (`mat_...`): rate + list + read consent.
//! Users: rate + set consent.

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
    MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState, UserOrInternal,
};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::facade::AgentFeedbackFacade;
use crate::domain::model::{
    ConsentRecord, FeedbackError, FeedbackSharingMode, MessageRating, RatingValue,
};
use crate::domain::service::FeedbackService;

/// Router state.
pub struct FeedbackRouterState<A, I, Auth> {
    /// Agent-facing facade.
    pub facade: Arc<AgentFeedbackFacade<A>>,
    /// Identity service.
    pub identity: Arc<I>,
    /// Authorization state.
    pub authorization_state: MacroAuthorizationState<Auth>,
}

impl<A, I, Auth> Clone for FeedbackRouterState<A, I, Auth> {
    fn clone(&self) -> Self {
        Self {
            facade: self.facade.clone(),
            identity: self.identity.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<A, I, Auth> FromRef<FeedbackRouterState<A, I, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &FeedbackRouterState<A, I, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Error body.
#[derive(Debug, Serialize, ToSchema)]
pub struct FeedbackErrorBody {
    /// Error description.
    pub error: String,
}

fn error_response(e: FeedbackError) -> Response {
    let status = match &e {
        FeedbackError::NotFound => StatusCode::NOT_FOUND,
        FeedbackError::MissingScope { .. } => StatusCode::FORBIDDEN,
        FeedbackError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
        FeedbackError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(FeedbackErrorBody {
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
        Json(FeedbackErrorBody {
            error: "agent authentication failed".to_string(),
        }),
    )
        .into_response()
}

/// Rate body.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RateRequest {
    /// Ledger seq of the rated event.
    pub target_seq: i64,
    /// Rating.
    pub rating: RatingValue,
    /// Optional note.
    pub note: Option<String>,
}

/// Consent body.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ConsentRequest {
    /// Sharing mode.
    pub sharing_mode: FeedbackSharingMode,
}

/// Build the feedback router.
pub fn agent_feedback_router<A, I, Auth, S>(state: FeedbackRouterState<A, I, Auth>) -> Router<S>
where
    A: FeedbackService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
    S: Send + Sync + Clone + 'static,
{
    Router::new()
        .route(
            "/agent-feedback/{session_id}/ratings",
            post(agent_rate_handler::<A, I, Auth>).get(agent_list_ratings_handler::<A, I, Auth>),
        )
        .route(
            "/agent-feedback/{session_id}/consent",
            get(agent_get_consent_handler::<A, I, Auth>)
                .post(user_set_consent_handler::<A, I, Auth>),
        )
        .route(
            "/agent-feedback/{session_id}/user-ratings",
            post(user_rate_handler::<A, I, Auth>),
        )
        .with_state(state)
}

/// Rate a ledger event. Agent callers.
#[utoipa::path(
    post,
    path = "/agent-feedback/{session_id}/ratings",
    params(("session_id" = Uuid, Path)),
    request_body = RateRequest,
    responses((status = 200, description = "The rating", body = MessageRating)),
    tag = "feedback"
)]
#[tracing::instrument(skip_all)]
pub async fn agent_rate_handler<A, I, Auth>(
    State(state): State<FeedbackRouterState<A, I, Auth>>,
    bearer: AgentBearer,
    Path(session_id): Path<Uuid>,
    Json(body): Json<RateRequest>,
) -> Response
where
    A: FeedbackService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state
        .facade
        .rate(&agent, session_id, body.target_seq, body.rating, body.note)
        .await
    {
        Ok(rating) => Json(rating).into_response(),
        Err(e) => error_response(e),
    }
}

/// List sidecar ratings. Agent callers.
#[utoipa::path(
    get,
    path = "/agent-feedback/{session_id}/ratings",
    params(("session_id" = Uuid, Path)),
    responses((status = 200, description = "Ratings", body = Vec<MessageRating>)),
    tag = "feedback"
)]
#[tracing::instrument(skip_all)]
pub async fn agent_list_ratings_handler<A, I, Auth>(
    State(state): State<FeedbackRouterState<A, I, Auth>>,
    bearer: AgentBearer,
    Path(session_id): Path<Uuid>,
) -> Response
where
    A: FeedbackService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state.facade.list_ratings(&agent, session_id).await {
        Ok(ratings) => Json(ratings).into_response(),
        Err(e) => error_response(e),
    }
}

/// Read session consent. Agent callers.
#[utoipa::path(
    get,
    path = "/agent-feedback/{session_id}/consent",
    params(("session_id" = Uuid, Path)),
    responses((status = 200, description = "Consent", body = ConsentRecord)),
    tag = "feedback"
)]
#[tracing::instrument(skip_all)]
pub async fn agent_get_consent_handler<A, I, Auth>(
    State(state): State<FeedbackRouterState<A, I, Auth>>,
    bearer: AgentBearer,
    Path(session_id): Path<Uuid>,
) -> Response
where
    A: FeedbackService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state.facade.get_consent(&agent, session_id).await {
        Ok(consent) => Json(consent).into_response(),
        Err(e) => error_response(e),
    }
}

/// Set sharing consent. User callers.
#[utoipa::path(
    post,
    path = "/agent-feedback/{session_id}/consent",
    params(("session_id" = Uuid, Path)),
    request_body = ConsentRequest,
    responses((status = 200, description = "Consent", body = ConsentRecord)),
    tag = "feedback"
)]
#[tracing::instrument(skip_all)]
pub async fn user_set_consent_handler<A, I, Auth>(
    State(state): State<FeedbackRouterState<A, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(session_id): Path<Uuid>,
    Json(body): Json<ConsentRequest>,
) -> Response
where
    A: FeedbackService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let set_by = user.authorization.user.macro_user_id.as_ref().to_string();
    match state
        .facade
        .set_consent_as_user(session_id, None, body.sharing_mode, set_by)
        .await
    {
        Ok(consent) => Json(consent).into_response(),
        Err(e) => error_response(e),
    }
}

/// Rate a ledger event. User callers.
#[utoipa::path(
    post,
    path = "/agent-feedback/{session_id}/user-ratings",
    params(("session_id" = Uuid, Path)),
    request_body = RateRequest,
    responses((status = 200, description = "The rating", body = MessageRating)),
    tag = "feedback"
)]
#[tracing::instrument(skip_all)]
pub async fn user_rate_handler<A, I, Auth>(
    State(state): State<FeedbackRouterState<A, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(session_id): Path<Uuid>,
    Json(body): Json<RateRequest>,
) -> Response
where
    A: FeedbackService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let rated_by = user.authorization.user.macro_user_id.as_ref().to_string();
    match state
        .facade
        .rate_as_user(
            session_id,
            body.target_seq,
            body.rating,
            body.note,
            rated_by,
        )
        .await
    {
        Ok(rating) => Json(rating).into_response(),
        Err(e) => error_response(e),
    }
}
