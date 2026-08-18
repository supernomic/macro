//! Outbound WorkOS HTTP client.

use std::net::IpAddr;
use std::time::Duration;

use crate::error::WorkOsClientError;
use crate::ids::WorkOsOrganizationId;
use crate::models::{
    AuthenticateWithCodeRequest, AuthenticateWithCodeResponse, AuthorizationUrlParams,
    CreateOrganizationRequest, DomainData, GeneratePortalLinkRequest, OrganizationListResponse,
    PortalIntent, PortalLink, WorkOsOrganization,
};

const DEFAULT_API_BASE_URL: &str = "https://api.workos.com";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Client for WorkOS User Management, Organizations, and Admin Portal.
///
/// When credentials are missing the client is a no-op, matching
/// [`loops_client`](../loops_client) so local environments can run without WorkOS.
#[derive(Clone)]
pub struct WorkOsClient {
    inner: Option<Inner>,
}

#[derive(Clone)]
struct Inner {
    http: reqwest::Client,
    api_key: String,
    client_id: String,
    redirect_uri: String,
    api_base_url: String,
}

impl WorkOsClient {
    /// Creates a WorkOS client.
    pub fn new(api_key: String, client_id: String, redirect_uri: String) -> Self {
        Self::new_with_base_url(
            api_key,
            client_id,
            redirect_uri,
            DEFAULT_API_BASE_URL.to_string(),
        )
    }

    /// Creates a WorkOS client against a custom API base URL (tests).
    pub fn new_with_base_url(
        api_key: String,
        client_id: String,
        redirect_uri: String,
        api_base_url: String,
    ) -> Self {
        Self {
            inner: Some(Inner {
                http: reqwest::Client::builder()
                    .timeout(REQUEST_TIMEOUT)
                    .build()
                    .expect("reqwest client should build"),
                api_key,
                client_id,
                redirect_uri,
                api_base_url,
            }),
        }
    }

    /// Creates a no-op client (WorkOS not configured).
    pub fn noop() -> Self {
        Self { inner: None }
    }

    /// Whether this client can call WorkOS.
    pub fn is_configured(&self) -> bool {
        self.inner.is_some()
    }

    fn inner(&self) -> Result<&Inner, WorkOsClientError> {
        self.inner.as_ref().ok_or(WorkOsClientError::NotConfigured)
    }

    /// AuthKit authorization URL. Companies enter their email; WorkOS matches
    /// the domain to an organization and routes them through that org's SSO.
    #[tracing::instrument(skip(self, params), err)]
    pub fn authorization_url(
        &self,
        params: AuthorizationUrlParams<'_>,
    ) -> Result<String, WorkOsClientError> {
        let inner = self.inner()?;
        let mut url =
            url::Url::parse(&format!("{}/user_management/authorize", inner.api_base_url))?;

        {
            let mut query = url.query_pairs_mut();
            query.append_pair("response_type", "code");
            query.append_pair("client_id", &inner.client_id);
            query.append_pair("redirect_uri", &inner.redirect_uri);
            query.append_pair("provider", "authkit");
            if let Some(state) = params.state {
                query.append_pair("state", state);
            }
            if let Some(login_hint) = params.login_hint {
                query.append_pair("login_hint", login_hint);
            }
            if let Some(organization_id) = params.organization_id {
                query.append_pair("organization_id", organization_id.as_str());
            }
            if let Some(screen_hint) = params.screen_hint {
                query.append_pair("screen_hint", screen_hint.as_str());
            }
        }

        Ok(url.into())
    }

    /// Exchange an AuthKit authorization code for the WorkOS user and matched organization.
    #[tracing::instrument(skip(self, code, user_agent), err)]
    pub async fn authenticate_with_code(
        &self,
        code: &str,
        ip_address: Option<IpAddr>,
        user_agent: Option<&str>,
    ) -> Result<AuthenticateWithCodeResponse, WorkOsClientError> {
        let inner = self.inner()?;
        let ip_address = ip_address.map(|ip| ip.to_string());
        let body = AuthenticateWithCodeRequest {
            client_id: &inner.client_id,
            client_secret: &inner.api_key,
            grant_type: "authorization_code",
            code,
            ip_address: ip_address.as_deref(),
            user_agent,
        };

        let response = inner
            .http
            .post(format!(
                "{}/user_management/authenticate",
                inner.api_base_url
            ))
            .json(&body)
            .send()
            .await?;

        parse_json(response).await
    }

    /// Create a WorkOS organization so a customer company can later sign in via AuthKit.
    #[tracing::instrument(skip(self), err)]
    pub async fn create_organization(
        &self,
        name: &str,
        domains: &[&str],
    ) -> Result<WorkOsOrganization, WorkOsClientError> {
        let inner = self.inner()?;
        let body = CreateOrganizationRequest {
            name,
            domain_data: domains
                .iter()
                .map(|domain| DomainData {
                    domain,
                    state: "pending",
                })
                .collect(),
        };

        let response = inner
            .http
            .post(format!("{}/organizations", inner.api_base_url))
            .bearer_auth(&inner.api_key)
            .json(&body)
            .send()
            .await?;

        parse_json(response).await
    }

    /// Look up WorkOS organizations that claim `domain`.
    #[tracing::instrument(skip(self), err)]
    pub async fn list_organizations_by_domain(
        &self,
        domain: &str,
    ) -> Result<Vec<WorkOsOrganization>, WorkOsClientError> {
        let inner = self.inner()?;
        let response = inner
            .http
            .get(format!("{}/organizations", inner.api_base_url))
            .bearer_auth(&inner.api_key)
            .query(&[("domains", domain)])
            .send()
            .await?;

        let list: OrganizationListResponse = parse_json(response).await?;
        Ok(list.data)
    }

    /// Generate a one-time Admin Portal link so a company can configure SSO.
    #[tracing::instrument(skip(self), err)]
    pub async fn generate_portal_link(
        &self,
        organization_id: &WorkOsOrganizationId,
        intent: PortalIntent,
    ) -> Result<PortalLink, WorkOsClientError> {
        let inner = self.inner()?;
        let body = GeneratePortalLinkRequest {
            organization: organization_id.as_str(),
            intent: intent.as_str(),
        };

        let response = inner
            .http
            .post(format!("{}/portal/generate_link", inner.api_base_url))
            .bearer_auth(&inner.api_key)
            .json(&body)
            .send()
            .await?;

        parse_json(response).await
    }
}

async fn parse_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, WorkOsClientError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response.json().await?);
    }

    let message = response
        .text()
        .await
        .unwrap_or_else(|_| "unable to read WorkOS error body".to_string());
    Err(WorkOsClientError::Api { status, message })
}
