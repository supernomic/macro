use crate::api::context::{ApiContext, AuthorizationService};
use authentication_service::service::workos::{
    GenerateWorkOsPortalLinkError, generate_sso_portal_link,
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use macro_authorization::{MacroAuthorizationExtractor, UserOrInternal};
use model::response::ErrorResponse;

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkOsPortalResponse {
    /// One-time WorkOS Admin Portal URL for SSO setup.
    pub url: String,
    /// WorkOS organization id linked to the caller's Macro organization.
    pub workos_organization_id: String,
}

#[derive(thiserror::Error, Debug)]
pub enum WorkOsPortalError {
    #[error("WorkOS is not configured")]
    NotConfigured,
    #[error("user is not in an organization")]
    NoOrganization,
    #[error("forbidden")]
    Forbidden,
    #[error("internal error")]
    Internal,
}

impl From<GenerateWorkOsPortalLinkError> for WorkOsPortalError {
    fn from(error: GenerateWorkOsPortalLinkError) -> Self {
        match error {
            GenerateWorkOsPortalLinkError::NotConfigured => Self::NotConfigured,
            GenerateWorkOsPortalLinkError::NoOrganization => Self::NoOrganization,
            GenerateWorkOsPortalLinkError::Forbidden => Self::Forbidden,
            GenerateWorkOsPortalLinkError::WorkOs(error) => {
                tracing::error!(error=?error, "WorkOS API error generating portal link");
                Self::Internal
            }
            GenerateWorkOsPortalLinkError::Internal(error) => {
                tracing::error!(error=?error, "internal error generating WorkOS portal link");
                Self::Internal
            }
        }
    }
}

impl IntoResponse for WorkOsPortalError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            WorkOsPortalError::NotConfigured => {
                (StatusCode::SERVICE_UNAVAILABLE, "WorkOS is not configured")
            }
            WorkOsPortalError::NoOrganization => {
                (StatusCode::BAD_REQUEST, "user is not in an organization")
            }
            WorkOsPortalError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            WorkOsPortalError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
        };
        (
            status,
            Json(ErrorResponse {
                message: message.into(),
            }),
        )
            .into_response()
    }
}

/// Creates (or reuses) a WorkOS organization for the caller's Macro tenant and
/// returns an Admin Portal link so that company can configure SSO against us.
#[utoipa::path(
        post,
        path = "/user/workos/portal",
        operation_id = "generate_workos_portal_link",
        responses(
            (status = 200, body = WorkOsPortalResponse),
            (status = 400, body = ErrorResponse),
            (status = 401, body = String),
            (status = 403, body = ErrorResponse),
            (status = 503, body = ErrorResponse),
        )
    )]
#[tracing::instrument(skip(ctx, user_context), err, fields(user_id=%user_context.authorization.user.user_context.user_id))]
pub async fn handler(
    State(ctx): State<ApiContext>,
    user_context: MacroAuthorizationExtractor<AuthorizationService, UserOrInternal>,
) -> Result<Json<WorkOsPortalResponse>, WorkOsPortalError> {
    let link = generate_sso_portal_link(
        &ctx.db,
        &ctx.workos_client,
        ctx.user_roles_and_permissions_service.as_ref(),
        &user_context.authorization.user.macro_user_id,
        user_context.authorization.user.user_context.organization_id,
    )
    .await?;

    Ok(Json(WorkOsPortalResponse {
        url: link.url,
        workos_organization_id: link.workos_organization_id.to_string(),
    }))
}
