use crate::api::context::ApiContext;
use crate::api::login::sso::{is_allowed_original_url, redact_original_url_for_logging};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use macro_env::Environment;
use model::response::ErrorResponse;
use serde_utils::urlencode::UrlEncoded;
use tower_cookies::Cookies;
use url::Url;
use utoipa::ToSchema;
use workos_client::{AuthorizationUrlParams, ScreenHint};

use crate::api::utils::{
    append_signed_up_param_if_new_user, create_access_token_cookie, create_refresh_token_cookie,
    default_redirect_url, generate_session_code, spawn_first_inbox_provision,
};
use authentication_service::service::workos::complete_workos_login;
use macro_middleware::tracking::ClientIp;

#[derive(Clone, serde::Serialize, serde::Deserialize, ToSchema, Debug, Default)]
pub struct WorkOsLoginState {
    #[schema(value_type = Option<String>)]
    pub original_url: Option<Url>,
    pub is_mobile: bool,
    pub referral_code: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct WorkOsLoginQueryParams {
    login_hint: Option<String>,
    original_url: Option<UrlEncoded<Url>>,
    #[serde(default)]
    is_mobile: bool,
    referral_code: Option<String>,
    #[serde(default)]
    signup: bool,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct WorkOsCallbackQueryParams {
    code: Option<String>,
    state: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

/// Initiates a WorkOS AuthKit login. Customer companies sign in here and are
/// matched to an organization in our WorkOS environment.
#[utoipa::path(
        get,
        path = "/login/workos",
        operation_id = "workos_login",
        params(
            ("login_hint" = String, Query, description = "**OPTIONAL**. Prefill the AuthKit email field."),
            ("original_url" = String, Query, description = "**OPTIONAL**. The original url you came from."),
            ("is_mobile" = String, Query, description = "**OPTIONAL**. If the authentication request is from a mobile device."),
            ("referral_code" = String, Query, description = "**OPTIONAL**. If the user opened a link with a referral code."),
            ("signup" = bool, Query, description = "**OPTIONAL**. Show the AuthKit sign-up screen first."),
        ),
        responses(
            (status = 307),
            (status = 400, body=ErrorResponse),
            (status = 503, body=ErrorResponse),
        )
    )]
#[tracing::instrument(skip(ctx, query), fields(is_mobile = query.is_mobile, signup = query.signup))]
pub async fn login_handler(
    State(ctx): State<ApiContext>,
    query: Query<WorkOsLoginQueryParams>,
) -> Result<Response, Response> {
    if !ctx.workos_client.is_configured() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                message: "WorkOS is not configured".into(),
            }),
        )
            .into_response());
    }

    let Query(WorkOsLoginQueryParams {
        login_hint,
        original_url,
        is_mobile,
        referral_code,
        signup,
    }) = query;

    let original_url = original_url.map(|url| url.0);
    if let Some(url) = original_url
        .as_ref()
        .filter(|url| !is_allowed_original_url(url))
    {
        let redacted_url = redact_original_url_for_logging(url);
        tracing::error!(
            auth_handoff_failure = "original_url_rejected",
            original_url = %redacted_url,
            "original_url is not allowed"
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                message: "provided original_url is not allowed".into(),
            }),
        )
            .into_response());
    }

    let state = WorkOsLoginState {
        original_url,
        is_mobile,
        referral_code,
    };
    let state = serde_json::to_string(&state).map_err(|e| {
        tracing::error!(error=?e, "unable to serialize WorkOS state");
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                message: "unable to serialize state".into(),
            }),
        )
            .into_response()
    })?;

    let url = ctx
        .workos_client
        .authorization_url(AuthorizationUrlParams {
            state: Some(&state),
            login_hint: login_hint.as_deref(),
            organization_id: None,
            screen_hint: Some(if signup {
                ScreenHint::SignUp
            } else {
                ScreenHint::SignIn
            }),
        })
        .map_err(|e| {
            tracing::error!(error=?e, "unable to construct WorkOS authorization url");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    message: "unable to start WorkOS login".into(),
                }),
            )
                .into_response()
        })?;

    Ok(Redirect::temporary(&url).into_response())
}

/// Completes a WorkOS AuthKit login and issues Macro session cookies.
#[utoipa::path(
        get,
        path = "/login/workos/callback",
        operation_id = "workos_callback",
        responses(
            (status = 200),
            (status = 307),
            (status = 400, body=ErrorResponse),
            (status = 500, body=ErrorResponse),
            (status = 503, body=ErrorResponse),
        )
    )]
#[tracing::instrument(skip(ctx, cookies, query, ip_context, headers))]
pub async fn callback_handler(
    State(ctx): State<ApiContext>,
    cookies: Cookies,
    ip_context: ClientIp,
    headers: axum::http::HeaderMap,
    query: Query<WorkOsCallbackQueryParams>,
) -> Result<Response, Response> {
    if !ctx.workos_client.is_configured() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                message: "WorkOS is not configured".into(),
            }),
        )
            .into_response());
    }

    let Query(WorkOsCallbackQueryParams {
        code,
        state,
        error,
        error_description,
    }) = query;

    if let Some(error) = error {
        tracing::warn!(
            error,
            error_description,
            "WorkOS callback returned an error"
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                message: "Sign-in failed. Please try again or contact support.".into(),
            }),
        )
            .into_response());
    }

    let code = code.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                message: "WorkOS callback is missing an authorization code".into(),
            }),
        )
            .into_response()
    })?;

    let state: WorkOsLoginState = match state.as_deref() {
        Some(state) => serde_json::from_str(state).map_err(|e| {
            tracing::error!(error=?e, "unable to deserialize WorkOS state");
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    message: "unable to deserialize state".into(),
                }),
            )
                .into_response()
        })?,
        None => WorkOsLoginState::default(),
    };

    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|value| value.to_str().ok());

    let authenticated = ctx
        .workos_client
        .authenticate_with_code(&code, Some(ip_context.origin_ip()), user_agent)
        .await
        .map_err(|e| {
            tracing::error!(error=?e, "unable to authenticate WorkOS code");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    message: "unable to complete WorkOS login".into(),
                }),
            )
                .into_response()
        })?;

    let mut url = if let Some(original_url) = &state.original_url {
        original_url.clone()
    } else {
        default_redirect_url()
    };

    let (access_token, refresh_token) = complete_workos_login(
        &ctx.db,
        &ctx.auth_client,
        &authenticated,
        ip_context.origin_ip(),
        url.as_str(),
    )
    .await
    .map_err(|e| {
        tracing::error!(error=?e, "unable to complete WorkOS login");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                message: "unable to complete WorkOS login".into(),
            }),
        )
            .into_response()
    })?;

    if state.is_mobile {
        let session_code = generate_session_code();
        let filtered: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, _)| k != "token")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        url.query_pairs_mut().clear().extend_pairs(filtered);
        url.query_pairs_mut().append_pair("token", &session_code);

        ctx.macro_cache_client
            .set_mobile_login_session(&session_code, &refresh_token)
            .await
            .map_err(|e| {
                tracing::error!(error=?e, "unable to set mobile login session");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        message: "unable to store session code".into(),
                    }),
                )
                    .into_response()
            })?;
    }

    append_signed_up_param_if_new_user(
        &ctx.macro_cache_client,
        &authenticated.user.email.to_lowercase(),
        &mut url,
    )
    .await;

    cookies.add(create_access_token_cookie(&access_token));
    cookies.add(create_refresh_token_cookie(&refresh_token));
    spawn_first_inbox_provision(&ctx, &access_token);

    match Environment::new_or_prod() {
        Environment::Local => Ok(StatusCode::OK.into_response()),
        Environment::Production | Environment::Develop => {
            Ok(Redirect::to(url.as_str()).into_response())
        }
    }
}
