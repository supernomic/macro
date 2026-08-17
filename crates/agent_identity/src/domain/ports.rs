//! Ports for agent identity.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;

use super::model::{AgentApiToken, AgentKind, AgentPrincipal, MintedToken, Result, VerifiedAgent};

/// Request to create a new agent principal.
#[derive(Debug, Clone)]
pub struct CreatePrincipal {
    /// Organization scope; `None` for platform-level agents.
    pub org_id: Option<i32>,
    /// Stable machine slug, unique per org.
    pub slug: String,
    /// Human-readable name.
    pub display_name: String,
    /// The kind of agent.
    pub kind: AgentKind,
}

/// Request to mint a token for a principal.
#[derive(Debug, Clone)]
pub struct MintToken {
    /// The principal to mint for.
    pub principal_id: Uuid,
    /// Operator-facing label.
    pub name: String,
    /// Capability scopes to grant.
    pub scopes: Vec<String>,
    /// Optional expiry.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Storage port for principals and tokens.
pub trait AgentIdentityRepo: Send + Sync + 'static {
    /// Insert a principal. Must fail with
    /// [`super::model::IdentityError::SlugTaken`] on slug conflicts.
    fn insert_principal(
        &self,
        principal: &AgentPrincipal,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Fetch a principal by id.
    fn get_principal(
        &self,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<AgentPrincipal>>> + Send;

    /// Fetch a principal by org + slug.
    fn get_principal_by_slug(
        &self,
        org_id: Option<i32>,
        slug: &str,
    ) -> impl Future<Output = Result<Option<AgentPrincipal>>> + Send;

    /// List an org's principals.
    fn list_principals(
        &self,
        org_id: Option<i32>,
    ) -> impl Future<Output = Result<Vec<AgentPrincipal>>> + Send;

    /// Mark a principal disabled.
    fn disable_principal(&self, id: Uuid) -> impl Future<Output = Result<()>> + Send;

    /// Insert a token record.
    fn insert_token(&self, token: &AgentApiToken) -> impl Future<Output = Result<()>> + Send;

    /// Fetch a token by id.
    fn get_token(&self, id: Uuid) -> impl Future<Output = Result<Option<AgentApiToken>>> + Send;

    /// Mark a token revoked.
    fn revoke_token(&self, id: Uuid) -> impl Future<Output = Result<()>> + Send;

    /// Record token usage (best effort; used for hygiene reporting).
    fn touch_token(&self, id: Uuid) -> impl Future<Output = Result<()>> + Send;
}

/// Domain service exposed to inbound adapters.
pub trait AgentIdentityService: Send + Sync + 'static {
    /// Create a new agent principal.
    fn create_principal(
        &self,
        request: CreatePrincipal,
    ) -> impl Future<Output = Result<AgentPrincipal>> + Send;

    /// Fetch a principal by id.
    fn get_principal(
        &self,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<AgentPrincipal>>> + Send;

    /// List an org's principals.
    fn list_principals(
        &self,
        org_id: Option<i32>,
    ) -> impl Future<Output = Result<Vec<AgentPrincipal>>> + Send;

    /// Disable a principal (its tokens stop verifying).
    fn disable_principal(&self, id: Uuid) -> impl Future<Output = Result<()>> + Send;

    /// Mint a scoped token for a principal. The bearer string is returned
    /// once and never stored.
    fn mint_token(&self, request: MintToken) -> impl Future<Output = Result<MintedToken>> + Send;

    /// Revoke a token.
    fn revoke_token(&self, id: Uuid) -> impl Future<Output = Result<()>> + Send;

    /// Verify a bearer string, returning the authenticated principal and the
    /// token's scopes.
    fn verify_bearer(&self, bearer: &str) -> impl Future<Output = Result<VerifiedAgent>> + Send;

    /// The current time; injected for testability of expiry logic.
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
