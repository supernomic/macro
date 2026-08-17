//! HTTP surface for training export.

use std::sync::Arc;

use axum::extract::{FromRef, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use macro_authorization::{
    InternalOnly, MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::model::{ExportError, ExportJob, ProjectedEvent, Projection, SharingMode};
use crate::domain::service::ExportService;

/// Router state.
pub struct ExportRouterState<A, Auth> {
    /// Export service.
    pub service: Arc<A>,
    /// Authorization state.
    pub authorization_state: MacroAuthorizationState<Auth>,
}

impl<A, Auth> Clone for ExportRouterState<A, Auth> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<A, Auth> FromRef<ExportRouterState<A, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &ExportRouterState<A, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Error body.
#[derive(Debug, Serialize, ToSchema)]
pub struct ExportErrorBody {
    /// Error description.
    pub error: String,
}

fn error_response(e: ExportError) -> Response {
    let status = match &e {
        ExportError::NotFound => StatusCode::NOT_FOUND,
        ExportError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
        ExportError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ExportErrorBody {
            error: e.to_string(),
        }),
    )
        .into_response()
}

/// Run-export body.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RunExportRequest {
    /// Organization.
    pub org_id: Option<i32>,
    /// Projection.
    pub projection: Projection,
    /// Sharing mode.
    #[serde(default = "default_sharing")]
    pub sharing_mode: SharingMode,
    /// Optional composition pin.
    pub composition_id: Option<String>,
    /// Window start.
    pub from_occurred_at: Option<DateTime<Utc>>,
    /// Window end.
    pub to_occurred_at: Option<DateTime<Utc>>,
}

fn default_sharing() -> SharingMode {
    SharingMode::Full
}

/// Run-export response.
#[derive(Debug, Serialize, ToSchema)]
pub struct RunExportResponse {
    /// Job record.
    pub job: ExportJob,
    /// Projected rows (capped by the query limit).
    pub events: Vec<ProjectedEvent>,
}

/// Build the export router.
pub fn training_export_router<A, Auth, S>(state: ExportRouterState<A, Auth>) -> Router<S>
where
    A: ExportService,
    Auth: MacroAuthorizationService,
    S: Send + Sync + Clone + 'static,
{
    Router::new()
        .route("/training-export", post(run_export_handler::<A, Auth>))
        .with_state(state)
}

/// Run a training-export projection.
#[utoipa::path(
    post,
    path = "/training-export",
    request_body = RunExportRequest,
    responses((status = 200, description = "Job + projected events", body = RunExportResponse)),
    tag = "training"
)]
#[tracing::instrument(skip_all)]
pub async fn run_export_handler<A, Auth>(
    State(state): State<ExportRouterState<A, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Json(body): Json<RunExportRequest>,
) -> Response
where
    A: ExportService,
    Auth: MacroAuthorizationService,
{
    match state
        .service
        .run(
            body.org_id,
            body.projection,
            body.sharing_mode,
            body.composition_id,
            body.from_occurred_at,
            body.to_occurred_at,
        )
        .await
    {
        Ok((job, events)) => Json(RunExportResponse { job, events }).into_response(),
        Err(e) => error_response(e),
    }
}
