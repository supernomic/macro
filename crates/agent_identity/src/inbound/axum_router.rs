//! Admin HTTP surface for agent principals and tokens.
//!
//! Mutating operations (create/disable principals, mint/revoke tokens) are
//! restricted to internal service callers — provisioning flows and the
//! extension activation service. Reads accept users or internal callers so
//! admin UIs can list an org's agents.

use std::sync::Arc;

use axum::extract::{FromRef, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use macro_authorization::{
    InternalOnly, MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState,
    UserOrInternal,
};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::model::{AgentKind, AgentPrincipal, IdentityError, MintedToken};
use crate::domain::ports::{AgentIdentityService, CreatePrincipal, MintToken};

/// Router state for the agent identity admin surface.
pub struct AgentIdentityRouterState<T, Auth> {
    /// The identity service implementation.
    pub service: Arc<T>,
    /// The authorization state used by the request extractors.
    pub authorization_state: MacroAuthorizationState<Auth>,
}

impl<T, Auth> Clone for AgentIdentityRouterState<T, Auth> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<T, Auth> FromRef<AgentIdentityRouterState<T, Auth>> for Arc<T> {
    fn from_ref(state: &AgentIdentityRouterState<T, Auth>) -> Self {
        state.service.clone()
    }
}

impl<T, Auth> FromRef<AgentIdentityRouterState<T, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &AgentIdentityRouterState<T, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Request body for creating an agent principal.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePrincipalRequest {
    /// Organization scope; omit for platform-level agents.
    pub org_id: Option<i32>,
    /// Stable machine slug (lowercase, unique per org).
    pub slug: String,
    /// Human-readable display name.
    pub display_name: String,
    /// `super_agent` | `domain_agent` | `workflow_agent` | `extension`
    pub kind: String,
}

/// Response body describing an agent principal.
#[derive(Debug, Serialize, ToSchema)]
pub struct PrincipalResponse {
    /// Principal id.
    pub id: Uuid,
    /// Organization scope.
    pub org_id: Option<i32>,
    /// Machine slug.
    pub slug: String,
    /// Display name.
    pub display_name: String,
    /// Agent kind.
    pub kind: String,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Disabled time, when disabled.
    pub disabled_at: Option<DateTime<Utc>>,
}

impl From<AgentPrincipal> for PrincipalResponse {
    fn from(p: AgentPrincipal) -> Self {
        Self {
            id: p.id,
            org_id: p.org_id,
            slug: p.slug,
            display_name: p.display_name,
            kind: p.kind.as_str().to_string(),
            created_at: p.created_at,
            disabled_at: p.disabled_at,
        }
    }
}

/// Request body for minting a token.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MintTokenRequest {
    /// Operator-facing label for the token.
    pub name: String,
    /// Capability scopes to grant.
    pub scopes: Vec<String>,
    /// Optional expiry.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Response body for a freshly minted token. The bearer string is only ever
/// returned here.
#[derive(Debug, Serialize, ToSchema)]
pub struct MintTokenResponse {
    /// Token id.
    pub id: Uuid,
    /// The full bearer string. Shown exactly once.
    pub bearer: String,
    /// Scopes granted.
    pub scopes: Vec<String>,
    /// Optional expiry.
    pub expires_at: Option<DateTime<Utc>>,
}

impl From<MintedToken> for MintTokenResponse {
    fn from(t: MintedToken) -> Self {
        Self {
            id: t.id,
            bearer: t.bearer,
            scopes: t.scopes,
            expires_at: t.expires_at,
        }
    }
}

/// Error body for identity endpoints.
#[derive(Debug, Serialize, ToSchema)]
pub struct IdentityErrorBody {
    /// Error description.
    pub error: String,
}

fn error_response(e: IdentityError) -> Response {
    let status = match &e {
        IdentityError::InvalidRequest(_) | IdentityError::MalformedToken => StatusCode::BAD_REQUEST,
        IdentityError::SlugTaken => StatusCode::CONFLICT,
        IdentityError::PrincipalNotFound => StatusCode::NOT_FOUND,
        IdentityError::TokenRejected | IdentityError::MissingScope { .. } => {
            StatusCode::UNAUTHORIZED
        }
        IdentityError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    if status == StatusCode::INTERNAL_SERVER_ERROR {
        tracing::error!(error = ?e, "agent identity endpoint failed");
        return (
            status,
            Json(IdentityErrorBody {
                error: "internal error".to_string(),
            }),
        )
            .into_response();
    }
    (
        status,
        Json(IdentityErrorBody {
            error: e.to_string(),
        }),
    )
        .into_response()
}

/// Build the agent identity admin router.
pub fn agent_identity_router<T, Auth, S>(state: AgentIdentityRouterState<T, Auth>) -> Router<S>
where
    T: AgentIdentityService,
    Auth: MacroAuthorizationService,
    S: Send + Sync + Clone + 'static,
{
    Router::new()
        .route(
            "/agent-identity/principals",
            post(create_principal_handler::<T, Auth>).get(list_principals_handler::<T, Auth>),
        )
        .route(
            "/agent-identity/principals/{principal_id}/disable",
            post(disable_principal_handler::<T, Auth>),
        )
        .route(
            "/agent-identity/principals/{principal_id}/tokens",
            post(mint_token_handler::<T, Auth>),
        )
        .route(
            "/agent-identity/tokens/{token_id}/revoke",
            post(revoke_token_handler::<T, Auth>),
        )
        .with_state(state)
}

/// Create an agent principal. Internal callers only.
#[utoipa::path(
    post,
    path = "/agent-identity/principals",
    request_body = CreatePrincipalRequest,
    responses(
        (status = 200, description = "The created principal", body = PrincipalResponse),
        (status = 400, description = "Invalid request", body = IdentityErrorBody),
        (status = 409, description = "Slug already taken", body = IdentityErrorBody),
    ),
    tag = "agent-identity"
)]
#[tracing::instrument(skip(service, _caller, body))]
pub async fn create_principal_handler<T: AgentIdentityService, Auth: MacroAuthorizationService>(
    State(service): State<Arc<T>>,
    _caller: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Json(body): Json<CreatePrincipalRequest>,
) -> Response {
    let Some(kind) = AgentKind::parse(&body.kind) else {
        return error_response(IdentityError::InvalidRequest(format!(
            "unknown agent kind: {}",
            body.kind
        )));
    };
    match service
        .create_principal(CreatePrincipal {
            org_id: body.org_id,
            slug: body.slug,
            display_name: body.display_name,
            kind,
        })
        .await
    {
        Ok(principal) => Json(PrincipalResponse::from(principal)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Query parameters for listing principals.
#[derive(Debug, Deserialize)]
pub struct ListPrincipalsQuery {
    /// Organization to list principals for; omit for platform-level agents.
    pub org_id: Option<i32>,
}

/// List agent principals for an org.
#[utoipa::path(
    get,
    path = "/agent-identity/principals",
    responses(
        (status = 200, description = "Principals in the org", body = Vec<PrincipalResponse>),
    ),
    tag = "agent-identity"
)]
#[tracing::instrument(skip(service, _caller))]
pub async fn list_principals_handler<T: AgentIdentityService, Auth: MacroAuthorizationService>(
    State(service): State<Arc<T>>,
    _caller: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    axum::extract::Query(query): axum::extract::Query<ListPrincipalsQuery>,
) -> Response {
    match service.list_principals(query.org_id).await {
        Ok(principals) => Json(
            principals
                .into_iter()
                .map(PrincipalResponse::from)
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => error_response(e),
    }
}

/// Disable an agent principal (its tokens stop verifying). Internal callers
/// only.
#[utoipa::path(
    post,
    path = "/agent-identity/principals/{principal_id}/disable",
    responses(
        (status = 204, description = "Principal disabled"),
    ),
    tag = "agent-identity"
)]
#[tracing::instrument(skip(service, _caller))]
pub async fn disable_principal_handler<T: AgentIdentityService, Auth: MacroAuthorizationService>(
    State(service): State<Arc<T>>,
    _caller: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Path(principal_id): Path<Uuid>,
) -> Response {
    match service.disable_principal(principal_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error_response(e),
    }
}

/// Mint a scoped API token for a principal. Internal callers only.
#[utoipa::path(
    post,
    path = "/agent-identity/principals/{principal_id}/tokens",
    request_body = MintTokenRequest,
    responses(
        (status = 200, description = "The minted token (bearer shown once)", body = MintTokenResponse),
        (status = 400, description = "Invalid request", body = IdentityErrorBody),
        (status = 404, description = "Principal not found", body = IdentityErrorBody),
    ),
    tag = "agent-identity"
)]
#[tracing::instrument(skip(service, _caller, body))]
pub async fn mint_token_handler<T: AgentIdentityService, Auth: MacroAuthorizationService>(
    State(service): State<Arc<T>>,
    _caller: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Path(principal_id): Path<Uuid>,
    Json(body): Json<MintTokenRequest>,
) -> Response {
    match service
        .mint_token(MintToken {
            principal_id,
            name: body.name,
            scopes: body.scopes,
            expires_at: body.expires_at,
        })
        .await
    {
        Ok(minted) => Json(MintTokenResponse::from(minted)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Revoke an API token. Internal callers only.
#[utoipa::path(
    post,
    path = "/agent-identity/tokens/{token_id}/revoke",
    responses(
        (status = 204, description = "Token revoked"),
    ),
    tag = "agent-identity"
)]
#[tracing::instrument(skip(service, _caller))]
pub async fn revoke_token_handler<T: AgentIdentityService, Auth: MacroAuthorizationService>(
    State(service): State<Arc<T>>,
    _caller: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Path(token_id): Path<Uuid>,
) -> Response {
    match service.revoke_token(token_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error_response(e),
    }
}
