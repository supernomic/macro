//! HTTP surface for lifecycle connectors.

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

use crate::domain::model::{
    ConnectorAccount, ConnectorError, ConnectorRecord, IngestRecord, Provider,
};
use crate::domain::service::ConnectorService;

/// Router state.
pub struct ConnectorRouterState<A, Auth> {
    /// Connector service.
    pub service: Arc<A>,
    /// Authorization state.
    pub authorization_state: MacroAuthorizationState<Auth>,
}

impl<A, Auth> Clone for ConnectorRouterState<A, Auth> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<A, Auth> FromRef<ConnectorRouterState<A, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &ConnectorRouterState<A, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Error body.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConnectorErrorBody {
    /// Error description.
    pub error: String,
}

fn error_response(e: ConnectorError) -> Response {
    let status = match &e {
        ConnectorError::NotFound => StatusCode::NOT_FOUND,
        ConnectorError::MissingScope { .. } => StatusCode::FORBIDDEN,
        ConnectorError::InvalidRequest(_) | ConnectorError::Graph(_) => StatusCode::BAD_REQUEST,
        ConnectorError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ConnectorErrorBody {
            error: e.to_string(),
        }),
    )
        .into_response()
}

/// Register-account body.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterAccountRequest {
    /// Organization.
    pub org_id: Option<i32>,
    /// Provider.
    pub provider: Provider,
    /// Display name.
    pub display_name: String,
    /// Credential reference.
    pub credential_ref: String,
}

/// Ingest body.
#[derive(Debug, Deserialize, ToSchema)]
pub struct IngestRequest {
    /// Records.
    pub records: Vec<IngestRecord>,
    /// Cursor.
    pub cursor: Option<String>,
}

/// Build the connector router (internal callers: sync jobs).
pub fn lifecycle_connectors_router<A, Auth, S>(state: ConnectorRouterState<A, Auth>) -> Router<S>
where
    A: ConnectorService,
    Auth: MacroAuthorizationService,
    S: Send + Sync + Clone + 'static,
{
    Router::new()
        .route(
            "/lifecycle-connectors/accounts",
            post(register_account_handler::<A, Auth>),
        )
        .route(
            "/lifecycle-connectors/accounts/{id}/ingest",
            post(ingest_handler::<A, Auth>),
        )
        .with_state(state)
}

/// Register a provider account.
#[utoipa::path(
    post,
    path = "/lifecycle-connectors/accounts",
    request_body = RegisterAccountRequest,
    responses((status = 200, description = "The account", body = ConnectorAccount)),
    tag = "connectors"
)]
#[tracing::instrument(skip_all)]
pub async fn register_account_handler<A, Auth>(
    State(state): State<ConnectorRouterState<A, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Json(body): Json<RegisterAccountRequest>,
) -> Response
where
    A: ConnectorService,
    Auth: MacroAuthorizationService,
{
    match state
        .service
        .register_account(
            body.org_id,
            body.provider,
            body.display_name,
            body.credential_ref,
        )
        .await
    {
        Ok(account) => Json(account).into_response(),
        Err(e) => error_response(e),
    }
}

/// Ingest a batch of provider records.
#[utoipa::path(
    post,
    path = "/lifecycle-connectors/accounts/{id}/ingest",
    request_body = IngestRequest,
    responses((status = 200, description = "Stored records", body = Vec<ConnectorRecord>)),
    tag = "connectors"
)]
#[tracing::instrument(skip_all)]
pub async fn ingest_handler<A, Auth>(
    State(state): State<ConnectorRouterState<A, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Path(id): Path<Uuid>,
    Json(body): Json<IngestRequest>,
) -> Response
where
    A: ConnectorService,
    Auth: MacroAuthorizationService,
{
    match state.service.ingest(id, body.records, body.cursor).await {
        Ok(records) => Json(records).into_response(),
        Err(e) => error_response(e),
    }
}
