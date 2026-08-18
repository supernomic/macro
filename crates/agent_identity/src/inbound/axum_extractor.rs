//! Axum extractor for agent bearer tokens.
//!
//! Hosting services verify the bearer through their
//! [`crate::domain::ports::AgentIdentityService`] and pass the resulting
//! [`crate::domain::model::VerifiedAgent`] into domain calls. This extractor
//! only pulls the raw bearer string out of the transport; verification —
//! an authentication concern with storage access — happens in the handler's
//! service call so hosting routers stay generic over their service types.

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;

use crate::domain::model::TOKEN_PREFIX;

/// The raw agent bearer string from the `Authorization` header. Present only
/// when the header carries an agent token (`Bearer mat_...`), so routes can
/// distinguish agent callers from user JWTs.
#[derive(Debug, Clone)]
pub struct AgentBearer(pub String);

impl<S: Send + Sync> FromRequestParts<S> for AgentBearer {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or((StatusCode::UNAUTHORIZED, "missing authorization header"))?;

        let bearer = header
            .strip_prefix("Bearer ")
            .ok_or((StatusCode::UNAUTHORIZED, "expected bearer authorization"))?;

        if !bearer.starts_with(TOKEN_PREFIX) {
            return Err((StatusCode::UNAUTHORIZED, "not an agent token"));
        }

        Ok(AgentBearer(bearer.to_string()))
    }
}
