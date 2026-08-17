//! HTTP surface for tenant extensions.

use std::sync::Arc;

use axum::extract::{FromRef, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use macro_authorization::{
    InternalAuthorization, InternalOnly, MacroAuthorizationExtractor, MacroAuthorizationService,
    MacroAuthorizationState,
};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::model::{ExtensionError, RegisterExtension, TenantExtension};
use crate::domain::service::ExtensionService;

/// Router state.
pub struct ExtensionRouterState<A, Auth> {
    /// Extension service.
    pub service: Arc<A>,
    /// Authorization state.
    pub authorization_state: MacroAuthorizationState<Auth>,
}

impl<A, Auth> Clone for ExtensionRouterState<A, Auth> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<A, Auth> FromRef<ExtensionRouterState<A, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &ExtensionRouterState<A, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Error body.
#[derive(Debug, Serialize, ToSchema)]
pub struct ExtensionErrorBody {
    /// Error description.
    pub error: String,
}

fn error_response(e: ExtensionError) -> Response {
    let status = match &e {
        ExtensionError::NotFound => StatusCode::NOT_FOUND,
        ExtensionError::RetaggedRelease { .. } | ExtensionError::InvalidStatus(_) => {
            StatusCode::CONFLICT
        }
        ExtensionError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
        ExtensionError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ExtensionErrorBody {
            error: e.to_string(),
        }),
    )
        .into_response()
}

/// Internal-only actor: verified `acting_user`, never `.user`. Fallback `"internal"`.
fn internal_actor(auth: &InternalAuthorization) -> String {
    auth.acting_user
        .as_ref()
        .map(|u| u.macro_user_id.as_ref().to_string())
        .unwrap_or_else(|| "internal".to_string())
}

/// Register body.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterExtensionRequest {
    /// Organization.
    pub org_id: i32,
    /// Slug.
    pub slug: String,
    /// Display name.
    pub display_name: String,
    /// Version.
    pub version: String,
    /// SDK semver.
    pub sdk_semver: String,
    /// Manifest.
    pub manifest: serde_json::Value,
    /// Artifact hash.
    pub artifact_hash: String,
    /// Scopes.
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// Build the extensions router.
pub fn tenant_extensions_router<A, Auth, S>(state: ExtensionRouterState<A, Auth>) -> Router<S>
where
    A: ExtensionService,
    Auth: MacroAuthorizationService,
    S: Send + Sync + Clone + 'static,
{
    Router::new()
        .route(
            "/tenant-extensions",
            post(register_extension_handler::<A, Auth>),
        )
        .route(
            "/tenant-extensions/{id}/activate",
            post(activate_extension_handler::<A, Auth>),
        )
        .route(
            "/tenant-extensions/{id}/rollback",
            post(rollback_extension_handler::<A, Auth>),
        )
        .route(
            "/tenant-extensions/{id}/disable",
            post(disable_extension_handler::<A, Auth>),
        )
        .with_state(state)
}

/// Register a draft extension.
#[utoipa::path(
    post,
    path = "/tenant-extensions",
    request_body = RegisterExtensionRequest,
    responses((status = 200, description = "The extension", body = TenantExtension)),
    tag = "extensions"
)]
#[tracing::instrument(skip_all)]
pub async fn register_extension_handler<A, Auth>(
    State(state): State<ExtensionRouterState<A, Auth>>,
    internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Json(body): Json<RegisterExtensionRequest>,
) -> Response
where
    A: ExtensionService,
    Auth: MacroAuthorizationService,
{
    let actor = internal_actor(&internal.authorization);
    match state
        .service
        .register(
            body.org_id,
            RegisterExtension {
                slug: body.slug,
                display_name: body.display_name,
                version: body.version,
                sdk_semver: body.sdk_semver,
                manifest: body.manifest,
                artifact_hash: body.artifact_hash,
                scopes: body.scopes,
            },
            &actor,
        )
        .await
    {
        Ok(ext) => Json(ext).into_response(),
        Err(e) => error_response(e),
    }
}

/// Activate (candidate-set swap).
#[utoipa::path(
    post,
    path = "/tenant-extensions/{id}/activate",
    responses((status = 200, description = "Activated extension", body = TenantExtension)),
    tag = "extensions"
)]
#[tracing::instrument(skip_all)]
pub async fn activate_extension_handler<A, Auth>(
    State(state): State<ExtensionRouterState<A, Auth>>,
    internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Path(id): Path<Uuid>,
) -> Response
where
    A: ExtensionService,
    Auth: MacroAuthorizationService,
{
    let actor = internal_actor(&internal.authorization);
    match state.service.activate(id, &actor).await {
        Ok(ext) => Json(ext).into_response(),
        Err(e) => error_response(e),
    }
}

/// Rollback to the latest snapshot.
#[utoipa::path(
    post,
    path = "/tenant-extensions/{id}/rollback",
    responses((status = 200, description = "Rolled-back extension", body = TenantExtension)),
    tag = "extensions"
)]
#[tracing::instrument(skip_all)]
pub async fn rollback_extension_handler<A, Auth>(
    State(state): State<ExtensionRouterState<A, Auth>>,
    internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Path(id): Path<Uuid>,
) -> Response
where
    A: ExtensionService,
    Auth: MacroAuthorizationService,
{
    let actor = internal_actor(&internal.authorization);
    match state.service.rollback(id, &actor).await {
        Ok(ext) => Json(ext).into_response(),
        Err(e) => error_response(e),
    }
}

/// Delist kill switch.
#[utoipa::path(
    post,
    path = "/tenant-extensions/{id}/disable",
    responses((status = 200, description = "Disabled extension", body = TenantExtension)),
    tag = "extensions"
)]
#[tracing::instrument(skip_all)]
pub async fn disable_extension_handler<A, Auth>(
    State(state): State<ExtensionRouterState<A, Auth>>,
    internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Path(id): Path<Uuid>,
) -> Response
where
    A: ExtensionService,
    Auth: MacroAuthorizationService,
{
    let actor = internal_actor(&internal.authorization);
    match state.service.disable(id, &actor).await {
        Ok(ext) => Json(ext).into_response(),
        Err(e) => error_response(e),
    }
}
