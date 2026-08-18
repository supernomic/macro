//! WorkOS API request and response models.

use serde::{Deserialize, Serialize};

use crate::ids::{WorkOsOrganizationId, WorkOsUserId};

/// AuthKit screen to show first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenHint {
    /// Sign-in screen.
    SignIn,
    /// Sign-up screen.
    SignUp,
}

impl ScreenHint {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::SignIn => "sign-in",
            Self::SignUp => "sign-up",
        }
    }
}

/// Parameters for building an AuthKit authorization URL.
#[derive(Debug, Clone, Default)]
pub struct AuthorizationUrlParams<'a> {
    /// Opaque state returned on the callback.
    pub state: Option<&'a str>,
    /// Prefill the AuthKit email field and skip domain discovery when possible.
    pub login_hint: Option<&'a str>,
    /// Force a specific WorkOS organization (skips domain matching).
    pub organization_id: Option<&'a WorkOsOrganizationId>,
    /// Which AuthKit screen to show first.
    pub screen_hint: Option<ScreenHint>,
}

/// Authenticated WorkOS user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkOsUser {
    /// WorkOS user id.
    pub id: WorkOsUserId,
    /// Email address.
    pub email: String,
    /// Whether WorkOS considers the email verified.
    pub email_verified: bool,
    /// Given name.
    pub first_name: Option<String>,
    /// Family name.
    pub last_name: Option<String>,
}

/// Result of exchanging an AuthKit authorization code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticateWithCodeResponse {
    /// Authenticated user.
    pub user: WorkOsUser,
    /// Organization the user authenticated into, when AuthKit matched one.
    pub organization_id: Option<WorkOsOrganizationId>,
    /// WorkOS access token (unused by Macro; FusionAuth still issues our session).
    #[serde(default)]
    pub access_token: Option<String>,
}

/// A domain attached to a WorkOS organization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkOsOrganizationDomain {
    /// Domain name, e.g. `acme.com`.
    pub domain: String,
}

/// WorkOS organization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkOsOrganization {
    /// WorkOS organization id.
    pub id: WorkOsOrganizationId,
    /// Organization display name.
    pub name: String,
    /// Domains associated with the organization.
    #[serde(default)]
    pub domains: Vec<WorkOsOrganizationDomain>,
}

/// Admin Portal intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalIntent {
    /// Configure SSO / identity provider.
    Sso,
    /// Configure Directory Sync.
    Dsync,
    /// Verify a domain.
    DomainVerification,
}

impl PortalIntent {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Sso => "sso",
            Self::Dsync => "dsync",
            Self::DomainVerification => "domain_verification",
        }
    }
}

/// Generated Admin Portal link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortalLink {
    /// One-time Admin Portal URL.
    pub link: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OrganizationListResponse {
    pub data: Vec<WorkOsOrganization>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AuthenticateWithCodeRequest<'a> {
    pub client_id: &'a str,
    pub client_secret: &'a str,
    pub grant_type: &'a str,
    pub code: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CreateOrganizationRequest<'a> {
    pub name: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub domain_data: Vec<DomainData<'a>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DomainData<'a> {
    pub domain: &'a str,
    pub state: &'a str,
}

#[derive(Debug, Serialize)]
pub(crate) struct GeneratePortalLinkRequest<'a> {
    pub organization: &'a str,
    pub intent: &'a str,
}
