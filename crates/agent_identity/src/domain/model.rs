//! Domain models for agent principals and scoped API tokens.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Prefix of every agent API token.
pub const TOKEN_PREFIX: &str = "mat_";

/// The kind of agent a principal represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    /// The general-purpose super agent.
    SuperAgent,
    /// A domain agent (techops, security, people, procurement, ...).
    DomainAgent,
    /// A scheduled / background workflow agent.
    WorkflowAgent,
    /// A tenant-authored extension acting as its own principal.
    Extension,
}

impl AgentKind {
    /// Stable string used for storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentKind::SuperAgent => "super_agent",
            AgentKind::DomainAgent => "domain_agent",
            AgentKind::WorkflowAgent => "workflow_agent",
            AgentKind::Extension => "extension",
        }
    }

    /// Parse from the stored string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "super_agent" => Some(AgentKind::SuperAgent),
            "domain_agent" => Some(AgentKind::DomainAgent),
            "workflow_agent" => Some(AgentKind::WorkflowAgent),
            "extension" => Some(AgentKind::Extension),
            _ => None,
        }
    }
}

/// A first-class agent principal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPrincipal {
    /// Principal id (referenced by ledger events, sessions, tokens).
    pub id: Uuid,
    /// Organization scope; `None` for platform-level agents.
    pub org_id: Option<i32>,
    /// Stable machine slug, unique per org (e.g. `techops`).
    pub slug: String,
    /// Human-readable name shown in audit views and the Inbox.
    pub display_name: String,
    /// What kind of agent this is.
    pub kind: AgentKind,
    /// When the principal was created.
    pub created_at: DateTime<Utc>,
    /// When the principal was disabled, if it was. Disabled principals fail
    /// token verification.
    pub disabled_at: Option<DateTime<Utc>>,
}

/// A stored API token record (secret is stored only as a SHA-256 digest).
#[derive(Debug, Clone)]
pub struct AgentApiToken {
    /// Token id (embedded in the bearer string for O(1) lookup).
    pub id: Uuid,
    /// The principal this token authenticates.
    pub principal_id: Uuid,
    /// Operator-facing label (e.g. `techops-runtime`).
    pub name: String,
    /// SHA-256 digest of the token secret.
    pub secret_sha256: Vec<u8>,
    /// Capability scopes granted to this token.
    pub scopes: Vec<String>,
    /// When the token was created.
    pub created_at: DateTime<Utc>,
    /// Optional expiry.
    pub expires_at: Option<DateTime<Utc>>,
    /// When the token was revoked, if it was.
    pub revoked_at: Option<DateTime<Utc>>,
}

/// A freshly minted token. The bearer string is only available here, at mint
/// time; Macro stores just the digest.
#[derive(Debug, Clone, Serialize)]
pub struct MintedToken {
    /// Token id.
    pub id: Uuid,
    /// The full bearer string (`mat_<token_id>.<secret>`). Show once.
    pub bearer: String,
    /// Scopes granted.
    pub scopes: Vec<String>,
    /// Optional expiry.
    pub expires_at: Option<DateTime<Utc>>,
}

/// The result of verifying a bearer token: the authenticated principal plus
/// the token's scopes. This is what inbound adapters pass into domain
/// services as the agent's identity.
#[derive(Debug, Clone)]
pub struct VerifiedAgent {
    /// The authenticated principal.
    pub principal: AgentPrincipal,
    /// Token id used (for audit attribution).
    pub token_id: Uuid,
    /// Scopes granted to the presenting token.
    pub scopes: Vec<String>,
}

impl VerifiedAgent {
    /// Check that the token carries a scope satisfying `required`, returning
    /// a typed error otherwise.
    pub fn require_scope(&self, required: &str) -> Result<()> {
        if self.scopes.iter().any(|g| scope_allows(g, required)) {
            Ok(())
        } else {
            Err(IdentityError::MissingScope {
                required: required.to_string(),
            })
        }
    }
}

/// Whether the granted scope satisfies the required scope.
///
/// Grammar: colon-separated segments; a granted scope whose final segment is
/// `*` matches any required scope sharing the preceding prefix. Examples:
/// - granted `tool:search` allows required `tool:search`
/// - granted `tool:*` allows required `tool:search`
/// - granted `api:documents:*` allows `api:documents:read`
/// - granted `*` allows everything
pub fn scope_allows(granted: &str, required: &str) -> bool {
    if granted == required || granted == "*" {
        return true;
    }
    if let Some(prefix) = granted.strip_suffix(":*") {
        return required == prefix || required.starts_with(&format!("{prefix}:"));
    }
    false
}

/// Compute the SHA-256 digest of a token secret.
pub fn secret_digest(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}

/// Parse a bearer string into `(token_id, secret)`.
pub fn parse_bearer(bearer: &str) -> Result<(Uuid, &str)> {
    let rest = bearer
        .strip_prefix(TOKEN_PREFIX)
        .ok_or(IdentityError::MalformedToken)?;
    let (id_part, secret) = rest.split_once('.').ok_or(IdentityError::MalformedToken)?;
    let token_id = Uuid::try_parse(id_part).map_err(|_| IdentityError::MalformedToken)?;
    if secret.is_empty() {
        return Err(IdentityError::MalformedToken);
    }
    Ok((token_id, secret))
}

/// Errors returned by identity operations.
#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    /// The request is invalid.
    #[error("{0}")]
    InvalidRequest(String),
    /// The bearer string is not a valid agent token format.
    #[error("malformed agent token")]
    MalformedToken,
    /// The token is unknown, revoked, expired, or its principal is disabled.
    #[error("agent token rejected")]
    TokenRejected,
    /// The token lacks a required scope.
    #[error("missing required scope: {required}")]
    MissingScope {
        /// The scope that was required.
        required: String,
    },
    /// The principal was not found.
    #[error("agent principal not found")]
    PrincipalNotFound,
    /// A principal with the same slug already exists in the org.
    #[error("agent principal slug already exists")]
    SlugTaken,
    /// The storage backend failed.
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Result alias for identity operations.
pub type Result<T> = std::result::Result<T, IdentityError>;
