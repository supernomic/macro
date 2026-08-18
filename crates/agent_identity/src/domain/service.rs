//! Agent identity domain service.

#[cfg(test)]
mod test;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use macro_uuid::Uuid;

use super::model::{
    AgentApiToken, AgentPrincipal, IdentityError, MintedToken, Result, TOKEN_PREFIX, VerifiedAgent,
    parse_bearer, secret_digest,
};
use super::ports::{AgentIdentityRepo, AgentIdentityService, CreatePrincipal, MintToken};

/// Concrete identity service over an [`AgentIdentityRepo`].
#[derive(Debug, Clone)]
pub struct AgentIdentityServiceImpl<R> {
    repo: R,
}

impl<R> AgentIdentityServiceImpl<R> {
    /// Build a service over the given repo.
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
}

fn generate_secret() -> String {
    let bytes: [u8; 32] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Constant-time equality over two digests.
fn digest_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

impl<R: AgentIdentityRepo> AgentIdentityService for AgentIdentityServiceImpl<R> {
    #[tracing::instrument(skip(self, request), fields(slug = %request.slug), err)]
    async fn create_principal(&self, request: CreatePrincipal) -> Result<AgentPrincipal> {
        if request.slug.is_empty()
            || !request
                .slug
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(IdentityError::InvalidRequest(
                "slug must be non-empty lowercase [a-z0-9-_]".to_string(),
            ));
        }

        let principal = AgentPrincipal {
            id: macro_uuid::generate_uuid_v7(),
            org_id: request.org_id,
            slug: request.slug,
            display_name: request.display_name,
            kind: request.kind,
            created_at: Utc::now(),
            disabled_at: None,
        };
        self.repo.insert_principal(&principal).await?;
        Ok(principal)
    }

    #[tracing::instrument(skip(self), err)]
    async fn get_principal(&self, id: Uuid) -> Result<Option<AgentPrincipal>> {
        self.repo.get_principal(id).await
    }

    #[tracing::instrument(skip(self), err)]
    async fn list_principals(&self, org_id: Option<i32>) -> Result<Vec<AgentPrincipal>> {
        self.repo.list_principals(org_id).await
    }

    #[tracing::instrument(skip(self), err)]
    async fn disable_principal(&self, id: Uuid) -> Result<()> {
        self.repo.disable_principal(id).await
    }

    #[tracing::instrument(skip(self, request), fields(principal_id = %request.principal_id), err)]
    async fn mint_token(&self, request: MintToken) -> Result<MintedToken> {
        if request.scopes.is_empty() {
            return Err(IdentityError::InvalidRequest(
                "a token must carry at least one scope".to_string(),
            ));
        }
        let principal = self
            .repo
            .get_principal(request.principal_id)
            .await?
            .ok_or(IdentityError::PrincipalNotFound)?;
        if principal.disabled_at.is_some() {
            return Err(IdentityError::InvalidRequest(
                "cannot mint tokens for a disabled principal".to_string(),
            ));
        }

        let id = macro_uuid::generate_uuid_v7();
        let secret = generate_secret();
        let token = AgentApiToken {
            id,
            principal_id: request.principal_id,
            name: request.name,
            secret_sha256: secret_digest(&secret),
            scopes: request.scopes.clone(),
            created_at: Utc::now(),
            expires_at: request.expires_at,
            revoked_at: None,
        };
        self.repo.insert_token(&token).await?;

        Ok(MintedToken {
            id,
            bearer: format!("{TOKEN_PREFIX}{id}.{secret}"),
            scopes: request.scopes,
            expires_at: request.expires_at,
        })
    }

    #[tracing::instrument(skip(self), err)]
    async fn revoke_token(&self, id: Uuid) -> Result<()> {
        self.repo.revoke_token(id).await
    }

    #[tracing::instrument(skip_all, err)]
    async fn verify_bearer(&self, bearer: &str) -> Result<VerifiedAgent> {
        let (token_id, secret) = parse_bearer(bearer)?;
        let token = self
            .repo
            .get_token(token_id)
            .await?
            .ok_or(IdentityError::TokenRejected)?;

        if !digest_eq(&token.secret_sha256, &secret_digest(secret)) {
            return Err(IdentityError::TokenRejected);
        }
        if token.revoked_at.is_some() {
            return Err(IdentityError::TokenRejected);
        }
        if let Some(expires_at) = token.expires_at
            && expires_at <= Utc::now()
        {
            return Err(IdentityError::TokenRejected);
        }

        let principal = self
            .repo
            .get_principal(token.principal_id)
            .await?
            .ok_or(IdentityError::TokenRejected)?;
        if principal.disabled_at.is_some() {
            return Err(IdentityError::TokenRejected);
        }

        // Best-effort usage tracking; failures must not fail auth.
        let _ = self
            .repo
            .touch_token(token_id)
            .await
            .inspect_err(|e| tracing::warn!(error=?e, "failed to touch token"));

        Ok(VerifiedAgent {
            principal,
            token_id,
            scopes: token.scopes,
        })
    }
}
