//! WorkOS AuthKit login orchestration.
//!
//! Companies authenticate through our WorkOS environment. AuthKit matches their
//! email domain to a WorkOS organization; we then map that organization onto a
//! Macro tenant and mint a FusionAuth session so the rest of the product is unchanged.

use std::borrow::Cow;
use std::net::IpAddr;

use fusionauth::error::FusionAuthClientError;
use generic_email_domains::is_generic_email_domain;
use macro_user_id::user_id::MacroUserIdStr;
use roles_and_permissions::domain::{model::RoleId, port::UserRolesAndPermissionsService};
use sqlx::PgPool;
use workos_client::{
    AuthenticateWithCodeResponse, PortalIntent, WorkOsClient, WorkOsClientError,
    WorkOsOrganizationId,
};

use crate::generate_password::generate_random_password;

/// Sentinel `idp_id` returned when passwordless must redirect into WorkOS AuthKit.
pub const WORKOS_IDP_ID: &str = "workos";

/// One-time Admin Portal URL plus the WorkOS organization it belongs to.
#[derive(Debug, Clone)]
pub struct WorkOsPortalLink {
    /// One-time WorkOS Admin Portal URL for SSO setup.
    pub url: String,
    /// WorkOS organization id linked to the caller's Macro organization.
    pub workos_organization_id: WorkOsOrganizationId,
}

/// Failures while creating or reusing a WorkOS organization and Admin Portal link.
#[derive(thiserror::Error, Debug)]
pub enum GenerateWorkOsPortalLinkError {
    /// WorkOS is not configured in this environment.
    #[error("WorkOS is not configured")]
    NotConfigured,
    /// The caller has no Macro organization.
    #[error("user is not in an organization")]
    NoOrganization,
    /// The caller is not organization IT or a super admin.
    #[error("forbidden")]
    Forbidden,
    /// WorkOS API or identifier error.
    #[error(transparent)]
    WorkOs(#[from] WorkOsClientError),
    /// Database or other internal failure.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// Match a WorkOS-authenticated user to a Macro organization.
///
/// Prefer an explicit WorkOS organization id. If that id is not stored yet,
/// fall back to the existing email-domain match and link the two together so
/// later sign-ins go straight to the same tenant.
#[tracing::instrument(skip(db), err)]
pub async fn match_workos_organization(
    db: &PgPool,
    email: &str,
    workos_organization_id: Option<&WorkOsOrganizationId>,
) -> anyhow::Result<Option<i32>> {
    if let Some(workos_organization_id) = workos_organization_id {
        if let Some(organization_id) =
            macro_db_client::user::organization::get_organization_id_by_workos_id(
                db,
                workos_organization_id.as_str(),
            )
            .await?
        {
            return Ok(Some(organization_id));
        }

        if let Some(organization_id) =
            macro_db_client::user::organization::match_user_to_organization(db, email).await?
        {
            macro_db_client::user::organization::link_organization_to_workos(
                db,
                organization_id,
                workos_organization_id.as_str(),
            )
            .await?;
            return Ok(Some(organization_id));
        }

        return Ok(None);
    }

    macro_db_client::user::organization::match_user_to_organization(db, email).await
}

/// Ensure a FusionAuth user exists for the WorkOS identity, then return tokens.
#[tracing::instrument(skip(db, auth_client, authenticated), err)]
pub async fn complete_workos_login(
    db: &PgPool,
    auth_client: &fusionauth::FusionAuthClient,
    authenticated: &AuthenticateWithCodeResponse,
    ip_address: IpAddr,
    redirect_uri: &str,
) -> anyhow::Result<(String, String)> {
    let email = authenticated.user.email.to_lowercase();

    let organization_id =
        match_workos_organization(db, &email, authenticated.organization_id.as_ref()).await?;

    match auth_client.get_user_id_by_email(&email).await {
        Ok(_) => {}
        Err(FusionAuthClientError::UserDoesNotExist) => {
            auth_client
                .create_user(
                    fusionauth::user::create::User {
                        email: Cow::Borrowed(&email),
                        password: Cow::Owned(generate_random_password()),
                        username: None,
                    },
                    authenticated.user.email_verified,
                    ip_address,
                )
                .await?;
        }
        Err(error) => return Err(error.into()),
    }

    macro_db_client::user::organization::set_user_workos_user_id(
        db,
        &email,
        authenticated.user.id.as_str(),
    )
    .await?;

    if let Some(organization_id) = organization_id {
        macro_db_client::user::organization::assign_user_organization_if_unset(
            db,
            &email,
            organization_id,
        )
        .await?;
    }

    let code = auth_client
        .start_passwordless_login(&email, redirect_uri)
        .await?;
    let completed = auth_client.complete_passwordless_login(&code).await?;

    Ok((completed.token, completed.refresh_token))
}

/// Create or reuse a WorkOS organization for the caller's Macro tenant and
/// return an Admin Portal link so that company can configure SSO against us.
#[tracing::instrument(skip(db, workos_client, roles), err)]
pub async fn generate_sso_portal_link(
    db: &PgPool,
    workos_client: &WorkOsClient,
    roles: &impl UserRolesAndPermissionsService,
    macro_user_id: &MacroUserIdStr<'_>,
    organization_id: Option<i32>,
) -> Result<WorkOsPortalLink, GenerateWorkOsPortalLinkError> {
    if !workos_client.is_configured() {
        return Err(GenerateWorkOsPortalLinkError::NotConfigured);
    }

    let organization_id = organization_id.ok_or(GenerateWorkOsPortalLinkError::NoOrganization)?;

    let user_roles = roles
        .get_user_roles(macro_user_id)
        .await
        .map_err(|error| GenerateWorkOsPortalLinkError::Internal(error.into()))?;

    if !user_roles.contains(&RoleId::OrganizationIt) && !user_roles.contains(&RoleId::SuperAdmin) {
        return Err(GenerateWorkOsPortalLinkError::Forbidden);
    }

    let workos_organization_id =
        ensure_linked_workos_organization(db, workos_client, organization_id).await?;

    let portal = workos_client
        .generate_portal_link(&workos_organization_id, PortalIntent::Sso)
        .await?;

    Ok(WorkOsPortalLink {
        url: portal.link,
        workos_organization_id,
    })
}

async fn ensure_linked_workos_organization(
    db: &PgPool,
    workos_client: &WorkOsClient,
    organization_id: i32,
) -> Result<WorkOsOrganizationId, GenerateWorkOsPortalLinkError> {
    if let Some(existing) =
        macro_db_client::user::organization::get_workos_organization_id(db, organization_id).await?
    {
        return Ok(WorkOsOrganizationId::parse(existing).map_err(WorkOsClientError::from)?);
    }

    let name = macro_db_client::organization::get::organization::get_organization_name(
        db,
        organization_id,
    )
    .await?;
    let matches = macro_db_client::organization::get::organization_email_matches::get_organization_email_matches(
        db.clone(),
        organization_id,
    )
    .await
    .map_err(anyhow::Error::from)?;
    let domains = company_domains_from_email_matches(&matches);

    for domain in &domains {
        let found = workos_client.list_organizations_by_domain(domain).await?;
        if let Some(existing) = found.into_iter().next() {
            macro_db_client::user::organization::link_organization_to_workos(
                db,
                organization_id,
                existing.id.as_str(),
            )
            .await?;
            return Ok(existing.id);
        }
    }

    let created = workos_client.create_organization(&name, &domains).await?;
    macro_db_client::user::organization::link_organization_to_workos(
        db,
        organization_id,
        created.id.as_str(),
    )
    .await?;
    Ok(created.id)
}

/// Company domains from `OrganizationEmailMatches`, excluding addresses and
/// generic consumer providers so we do not claim `gmail.com` in WorkOS.
pub(crate) fn company_domains_from_email_matches(matches: &[String]) -> Vec<&str> {
    matches
        .iter()
        .map(String::as_str)
        .filter(|value| value.contains('.') && !value.contains('@'))
        .filter(|domain| !is_generic_email_domain(domain))
        .collect()
}

#[cfg(test)]
mod test;
