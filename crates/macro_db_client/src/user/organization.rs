use std::collections::HashSet;

#[cfg(test)]
mod test;

/// Matches a user to an organization based on their email
#[tracing::instrument(skip(db))]
pub async fn match_user_to_organization(
    db: &sqlx::Pool<sqlx::Postgres>,
    email: &str,
) -> anyhow::Result<Option<i32>> {
    let email_options = [
        email.to_string(),
        email.split('@').collect::<Vec<&str>>()[1].to_string(),
    ];

    let organization_id = sqlx::query!(
        r#"
        SELECT "organizationId" as organization_id
        FROM "OrganizationEmailMatches"
        WHERE email = ANY($1)
        "#,
        &email_options
    )
    .map(|row| row.organization_id)
    .fetch_optional(db)
    .await?;

    Ok(organization_id)
}

/// Matches a WorkOS organization to a Macro organization.
#[tracing::instrument(skip(db))]
pub async fn get_organization_id_by_workos_id(
    db: &sqlx::Pool<sqlx::Postgres>,
    workos_organization_id: &str,
) -> anyhow::Result<Option<i32>> {
    let organization_id = sqlx::query!(
        r#"
        SELECT id
        FROM "Organization"
        WHERE workos_organization_id = $1
        "#,
        workos_organization_id
    )
    .map(|row| row.id)
    .fetch_optional(db)
    .await?;

    Ok(organization_id)
}

/// Whether the email's matched Macro organization is linked to WorkOS SSO.
///
/// Used to send company users through AuthKit instead of email codes once the
/// tenant has been matched to our WorkOS environment.
#[tracing::instrument(skip(db))]
pub async fn organization_requires_workos_sso(
    db: &sqlx::Pool<sqlx::Postgres>,
    email: &str,
) -> anyhow::Result<bool> {
    let Some(organization_id) = match_user_to_organization(db, email).await? else {
        return Ok(false);
    };

    let linked = sqlx::query!(
        r#"
        SELECT workos_organization_id as "workos_organization_id?"
        FROM "Organization"
        WHERE id = $1
        "#,
        organization_id
    )
    .map(|row| row.workos_organization_id.is_some())
    .fetch_optional(db)
    .await?;

    Ok(linked.unwrap_or(false))
}

/// WorkOS organization id linked to a Macro organization, if any.
#[tracing::instrument(skip(db))]
pub async fn get_workos_organization_id(
    db: &sqlx::Pool<sqlx::Postgres>,
    organization_id: i32,
) -> anyhow::Result<Option<String>> {
    let workos_organization_id = sqlx::query!(
        r#"
        SELECT workos_organization_id as "workos_organization_id?"
        FROM "Organization"
        WHERE id = $1
        "#,
        organization_id
    )
    .map(|row| row.workos_organization_id)
    .fetch_optional(db)
    .await?;

    Ok(workos_organization_id.flatten())
}

/// Persist the WorkOS organization id on a Macro organization.
///
/// Idempotent: succeeds if the row is already linked to the same id.
#[tracing::instrument(skip(db))]
pub async fn link_organization_to_workos(
    db: &sqlx::Pool<sqlx::Postgres>,
    organization_id: i32,
    workos_organization_id: &str,
) -> anyhow::Result<()> {
    sqlx::query!(
        r#"
        UPDATE "Organization"
        SET workos_organization_id = $2
        WHERE id = $1
          AND (workos_organization_id IS NULL OR workos_organization_id = $2)
        "#,
        organization_id,
        workos_organization_id
    )
    .execute(db)
    .await?;

    Ok(())
}

/// Store the WorkOS user id on a Macro user.
#[tracing::instrument(skip(db))]
pub async fn set_user_workos_user_id(
    db: &sqlx::Pool<sqlx::Postgres>,
    email: &str,
    workos_user_id: &str,
) -> anyhow::Result<()> {
    sqlx::query!(
        r#"
        UPDATE "User"
        SET workos_user_id = $2
        WHERE email = $1
          AND (workos_user_id IS NULL OR workos_user_id = $2)
        "#,
        email,
        workos_user_id
    )
    .execute(db)
    .await?;

    Ok(())
}

/// Assign a user to a Macro organization when WorkOS matched one and they
/// are not already in an organization.
#[tracing::instrument(skip(db))]
pub async fn assign_user_organization_if_unset(
    db: &sqlx::Pool<sqlx::Postgres>,
    email: &str,
    organization_id: i32,
) -> anyhow::Result<()> {
    sqlx::query!(
        r#"
        UPDATE "User"
        SET "organizationId" = $2
        WHERE email = $1
          AND "organizationId" IS NULL
        "#,
        email,
        organization_id
    )
    .execute(db)
    .await?;

    Ok(())
}

/// Given a users email, returns the roles that user has in the organization
/// We require the email to check for potential `OrganizationIT` and `OrganizationBilling` roles
#[tracing::instrument(skip(db))]
pub async fn get_organization_roles_for_user(
    db: &sqlx::Pool<sqlx::Postgres>,
    organization_id: i32,
    email: &str,
) -> anyhow::Result<HashSet<String>> {
    let roles = sqlx::query!(
        r#"
        SELECT "roleId" as id
        FROM "RolesOnOrganizations"
        WHERE "organizationId" = $1
        "#,
        organization_id
    )
    .map(|row| row.id)
    .fetch_all(db)
    .await?;

    let mut roles: HashSet<String> = roles.into_iter().collect();

    // Check for organization it
    let it = sqlx::query!(
        r#"
        SELECT "organizationId" as id
        FROM "OrganizationIT"
        WHERE "email" = $1
        "#,
        email
    )
    .fetch_optional(db)
    .await?;

    if it.is_some() {
        tracing::debug!("user is an organization it contact");
        roles.insert("organization_it".to_string());
    }

    let billing = sqlx::query!(
        r#"
        SELECT "organizationId" as id
        FROM "OrganizationBilling"
        WHERE "email" = $1
        "#,
        email
    )
    .fetch_optional(db)
    .await?;

    if billing.is_some() {
        tracing::debug!("user is an organization billing contact");
        roles.insert("manage_organization_subscription".to_string());
    }

    Ok(roles)
}
