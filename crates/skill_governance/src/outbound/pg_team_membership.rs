//! Team membership adapter over MacroDB's `team_user` table.

use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::Result;
use crate::domain::ports::TeamMembershipPort;

/// Postgres-backed team membership lookups.
#[derive(Debug, Clone)]
pub struct PgTeamMembership {
    pool: PgPool,
}

impl PgTeamMembership {
    /// Build over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl TeamMembershipPort for PgTeamMembership {
    #[tracing::instrument(skip(self), err)]
    async fn user_teams(&self, user_id: &str) -> Result<Vec<Uuid>> {
        let rows = sqlx::query!(
            r#"
            SELECT team_id FROM team_user
            WHERE user_id = $1
            ORDER BY team_id
            "#,
            user_id,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.team_id).collect())
    }
}
