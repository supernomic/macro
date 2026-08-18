use super::*;
use sqlx::{Pool, Postgres};

#[sqlx::test(fixtures(path = "../../../fixtures", scripts("invite_user")))]
async fn match_user_to_organization_by_domain(pool: Pool<Postgres>) -> anyhow::Result<()> {
    let organization_id = match_user_to_organization(&pool, "ada@macro.com").await?;
    assert_eq!(organization_id, Some(1));
    Ok(())
}

#[sqlx::test(fixtures(path = "../../../fixtures", scripts("invite_user")))]
async fn workos_link_round_trip(pool: Pool<Postgres>) -> anyhow::Result<()> {
    assert!(!organization_requires_workos_sso(&pool, "ada@macro.com").await?);

    link_organization_to_workos(&pool, 1, "org_01H945H0YD4F97JN9MATX7BYAG").await?;

    assert_eq!(
        get_organization_id_by_workos_id(&pool, "org_01H945H0YD4F97JN9MATX7BYAG").await?,
        Some(1)
    );
    assert!(organization_requires_workos_sso(&pool, "ada@macro.com").await?);

    Ok(())
}

#[sqlx::test(fixtures(path = "../../../fixtures", scripts("invite_user")))]
async fn linking_the_same_workos_id_is_idempotent(pool: Pool<Postgres>) -> anyhow::Result<()> {
    link_organization_to_workos(&pool, 1, "org_01H945H0YD4F97JN9MATX7BYAG").await?;
    link_organization_to_workos(&pool, 1, "org_01H945H0YD4F97JN9MATX7BYAG").await?;
    assert_eq!(
        get_organization_id_by_workos_id(&pool, "org_01H945H0YD4F97JN9MATX7BYAG").await?,
        Some(1)
    );
    Ok(())
}

#[sqlx::test(fixtures(
    path = "../../../fixtures",
    scripts("invite_user", "basic_user_with_permissions")
))]
async fn assign_user_organization_and_workos_user_id(pool: Pool<Postgres>) -> anyhow::Result<()> {
    assign_user_organization_if_unset(&pool, "user@user.com", 1).await?;
    set_user_workos_user_id(&pool, "user@user.com", "user_01E4ZCR3C56J083X43JQXF3JK5").await?;

    let row = sqlx::query!(
        r#"
        SELECT "organizationId" as organization_id, workos_user_id as "workos_user_id?"
        FROM "User"
        WHERE email = $1
        "#,
        "user@user.com"
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(row.organization_id, Some(1));
    assert_eq!(
        row.workos_user_id.as_deref(),
        Some("user_01E4ZCR3C56J083X43JQXF3JK5")
    );

    assign_user_organization_if_unset(&pool, "user@user.com", 2).await?;
    let organization_id = sqlx::query_scalar!(
        r#"SELECT "organizationId" as "organization_id?" FROM "User" WHERE email = $1"#,
        "user@user.com"
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(organization_id, Some(1));

    Ok(())
}
