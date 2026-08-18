//! HTTP surface for the unified agent review inbox.
//!
//! User JWT only (same extractor as `/approvals/mine`). No agent bearer.

use std::sync::Arc;

use axum::extract::{FromRef, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use macro_authorization::{
    MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState, UserOrInternal,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::domain::model::{InboxError, InboxItem};
use crate::domain::service::InboxService;

/// Router state for the unified inbox surface.
pub struct InboxRouterState<I, Auth> {
    /// Aggregating inbox service.
    pub service: Arc<I>,
    /// Authorization state for user extraction.
    pub authorization_state: MacroAuthorizationState<Auth>,
}

impl<I, Auth> Clone for InboxRouterState<I, Auth> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<I, Auth> FromRef<InboxRouterState<I, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &InboxRouterState<I, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Error body for inbox endpoints.
#[derive(Debug, Serialize, ToSchema)]
pub struct InboxErrorBody {
    /// Error description.
    pub error: String,
}

/// Personal unified inbox.
#[derive(Debug, Serialize, ToSchema)]
pub struct UserInboxResponse {
    /// Cards from escalations, approvals, and skill proposals, newest first.
    pub items: Vec<InboxItem>,
}

fn error_response(e: InboxError) -> Response {
    let status = match &e {
        InboxError::Escalation(inner) => match inner {
            escalations::domain::model::EscalationError::Forbidden(_) => StatusCode::FORBIDDEN,
            escalations::domain::model::EscalationError::Database(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        },
        InboxError::Approval(inner) => match inner {
            approvals::domain::model::ApprovalError::Forbidden(_) => StatusCode::FORBIDDEN,
            approvals::domain::model::ApprovalError::Database(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        },
        InboxError::SkillProposal(inner) => match inner {
            skill_governance::domain::model::GovernanceError::Forbidden(_) => StatusCode::FORBIDDEN,
            skill_governance::domain::model::GovernanceError::Database(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        },
    };
    if status == StatusCode::INTERNAL_SERVER_ERROR {
        tracing::error!(error = ?e, "inbox endpoint failed");
        return (
            status,
            Json(InboxErrorBody {
                error: "internal error".to_string(),
            }),
        )
            .into_response();
    }
    (
        status,
        Json(InboxErrorBody {
            error: e.to_string(),
        }),
    )
        .into_response()
}

/// Build the unified inbox router.
pub fn inbox_router<I, Auth, S>(state: InboxRouterState<I, Auth>) -> Router<S>
where
    I: InboxService,
    Auth: MacroAuthorizationService,
    S: Send + Sync + Clone + 'static,
{
    Router::new()
        .route("/inbox/mine", get(list_my_inbox_handler::<I, Auth>))
        .with_state(state)
}

/// The caller's unified review inbox across escalations, approvals, and
/// skill proposals.
#[utoipa::path(
    get,
    path = "/inbox/mine",
    responses(
        (status = 200, description = "Unified inbox cards, newest first", body = UserInboxResponse),
    ),
    tag = "inbox"
)]
#[tracing::instrument(skip_all)]
pub async fn list_my_inbox_handler<I, Auth>(
    State(state): State<InboxRouterState<I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
) -> Response
where
    I: InboxService,
    Auth: MacroAuthorizationService,
{
    let user_id = user.authorization.user.macro_user_id.as_ref().to_string();
    match state.service.list_mine(&user_id).await {
        Ok(view) => Json(UserInboxResponse { items: view.items }).into_response(),
        Err(e) => error_response(e),
    }
}
