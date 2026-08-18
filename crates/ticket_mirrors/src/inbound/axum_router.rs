//! HTTP surface for ticket mirrors.

use std::sync::Arc;

use axum::extract::{FromRef, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use macro_authorization::{
    InternalOnly, MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState,
};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::model::{MirrorError, MirrorProvider, TicketMirror, UpsertMirror};
use crate::domain::service::MirrorService;

/// Router state.
pub struct MirrorRouterState<A, Auth> {
    /// Mirror service.
    pub service: Arc<A>,
    /// Authorization state.
    pub authorization_state: MacroAuthorizationState<Auth>,
}

impl<A, Auth> Clone for MirrorRouterState<A, Auth> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<A, Auth> FromRef<MirrorRouterState<A, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &MirrorRouterState<A, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Error body.
#[derive(Debug, Serialize, ToSchema)]
pub struct MirrorErrorBody {
    /// Error description.
    pub error: String,
}

fn error_response(e: MirrorError) -> Response {
    let status = match &e {
        MirrorError::NotFound => StatusCode::NOT_FOUND,
        MirrorError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
        MirrorError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(MirrorErrorBody {
            error: e.to_string(),
        }),
    )
        .into_response()
}

/// Upsert body.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpsertMirrorRequest {
    /// Organization.
    pub org_id: Option<i32>,
    /// Provider.
    pub provider: MirrorProvider,
    /// Native entity type.
    pub native_entity_type: String,
    /// Native entity id.
    pub native_entity_id: String,
    /// External id.
    pub foreign_id: String,
    /// Backlink.
    pub foreign_url: Option<String>,
    /// Summary.
    pub summary: String,
    /// Status.
    pub status: String,
}

/// Build the mirrors router.
pub fn ticket_mirrors_router<A, Auth, S>(state: MirrorRouterState<A, Auth>) -> Router<S>
where
    A: MirrorService,
    Auth: MacroAuthorizationService,
    S: Send + Sync + Clone + 'static,
{
    Router::new()
        .route("/ticket-mirrors", post(upsert_mirror_handler::<A, Auth>))
        .route(
            "/ticket-mirrors/{id}/disconnect",
            post(disconnect_mirror_handler::<A, Auth>),
        )
        .with_state(state)
}

/// Upsert a Zendesk/Jira mirror.
#[utoipa::path(
    post,
    path = "/ticket-mirrors",
    request_body = UpsertMirrorRequest,
    responses((status = 200, description = "The mirror", body = TicketMirror)),
    tag = "mirrors"
)]
#[tracing::instrument(skip_all)]
pub async fn upsert_mirror_handler<A, Auth>(
    State(state): State<MirrorRouterState<A, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Json(body): Json<UpsertMirrorRequest>,
) -> Response
where
    A: MirrorService,
    Auth: MacroAuthorizationService,
{
    match state
        .service
        .upsert(
            body.org_id,
            UpsertMirror {
                provider: body.provider,
                native_entity_type: body.native_entity_type,
                native_entity_id: body.native_entity_id,
                foreign_id: body.foreign_id,
                foreign_url: body.foreign_url,
                summary: body.summary,
                status: body.status,
            },
        )
        .await
    {
        Ok(mirror) => Json(mirror).into_response(),
        Err(e) => error_response(e),
    }
}

/// Disconnect a mirror without changing the native entity.
#[utoipa::path(
    post,
    path = "/ticket-mirrors/{id}/disconnect",
    responses((status = 200, description = "Disconnected mirror", body = TicketMirror)),
    tag = "mirrors"
)]
#[tracing::instrument(skip_all)]
pub async fn disconnect_mirror_handler<A, Auth>(
    State(state): State<MirrorRouterState<A, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Path(id): Path<Uuid>,
) -> Response
where
    A: MirrorService,
    Auth: MacroAuthorizationService,
{
    match state.service.disconnect(id).await {
        Ok(mirror) => Json(mirror).into_response(),
        Err(e) => error_response(e),
    }
}
