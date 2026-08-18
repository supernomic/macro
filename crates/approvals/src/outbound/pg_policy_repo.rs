//! Postgres implementation of the tool-policy storage port.

use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{ApprovalError, PolicyDecision, Result, ToolPolicy};
use crate::domain::ports::PolicyRepo;

/// Postgres-backed policy repo over the `approval_policies` table.
#[derive(Debug, Clone)]
pub struct PgPolicyRepo {
    pool: PgPool,
}

impl PgPolicyRepo {
    /// Build a repo over the given pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl PolicyRepo for PgPolicyRepo {
    #[tracing::instrument(skip(self), err)]
    async fn list_policies(&self, org_id: Option<i32>) -> Result<Vec<ToolPolicy>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, org_id, agent_slug, tool_name, decision,
                   approver_user_id, approver_team_id
            FROM approval_policies
            WHERE org_id IS NOT DISTINCT FROM $1
            ORDER BY agent_slug, tool_name
            "#,
            org_id,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                let decision = PolicyDecision::parse(&r.decision).ok_or_else(|| {
                    ApprovalError::InvalidRequest(format!("unknown decision: {}", r.decision))
                })?;
                Ok(ToolPolicy {
                    id: r.id,
                    org_id: r.org_id,
                    agent_slug: r.agent_slug,
                    tool_name: r.tool_name,
                    decision,
                    approver_user_id: r.approver_user_id,
                    approver_team_id: r.approver_team_id,
                })
            })
            .collect()
    }

    #[tracing::instrument(skip(self, policy), err)]
    async fn upsert_policy(&self, policy: &ToolPolicy) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO approval_policies (
                id, org_id, agent_slug, tool_name, decision,
                approver_user_id, approver_team_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (org_id, agent_slug, tool_name)
            DO UPDATE SET decision = EXCLUDED.decision,
                          approver_user_id = EXCLUDED.approver_user_id,
                          approver_team_id = EXCLUDED.approver_team_id,
                          updated_at = now()
            "#,
            policy.id,
            policy.org_id,
            policy.agent_slug,
            policy.tool_name,
            policy.decision.as_str(),
            policy.approver_user_id.as_deref(),
            policy.approver_team_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), err)]
    async fn delete_policy(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM approval_policies WHERE id = $1", id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
