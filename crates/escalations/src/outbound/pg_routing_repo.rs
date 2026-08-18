//! Postgres implementation of the routing configuration port.

use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{
    EscalationError, ExpertProfile, Priority, Result, RouteTarget, RoutingRule,
};
use crate::domain::ports::RoutingRepo;

/// Postgres-backed routing repo over the `escalation_routing_rules`,
/// `escalation_experts`, and `escalation_round_robin` tables.
#[derive(Debug, Clone)]
pub struct PgRoutingRepo {
    pool: PgPool,
}

impl PgRoutingRepo {
    /// Build a repo over the given pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct RuleRow {
    id: Uuid,
    org_id: Option<i32>,
    domain: String,
    position: i32,
    tags: Vec<String>,
    source_channel: Option<String>,
    min_priority: Option<String>,
    target_kind: String,
    target_user_id: Option<String>,
    target_team_id: Option<Uuid>,
}

fn row_to_rule(row: RuleRow) -> Result<RoutingRule> {
    let min_priority = row
        .min_priority
        .as_deref()
        .map(|p| {
            Priority::parse(p)
                .ok_or_else(|| EscalationError::InvalidRequest(format!("unknown priority: {p}")))
        })
        .transpose()?;
    let target = match row.target_kind.as_str() {
        "user" => RouteTarget::User {
            user_id: row.target_user_id.ok_or_else(|| {
                EscalationError::InvalidRequest("user rule without target_user_id".to_string())
            })?,
        },
        "team_queue" => RouteTarget::TeamQueue {
            team_id: row.target_team_id.ok_or_else(|| {
                EscalationError::InvalidRequest("team rule without target_team_id".to_string())
            })?,
        },
        "team_round_robin" => RouteTarget::TeamRoundRobin {
            team_id: row.target_team_id.ok_or_else(|| {
                EscalationError::InvalidRequest("team rule without target_team_id".to_string())
            })?,
        },
        other => {
            return Err(EscalationError::InvalidRequest(format!(
                "unknown target kind: {other}"
            )));
        }
    };
    Ok(RoutingRule {
        id: row.id,
        org_id: row.org_id,
        domain: row.domain,
        position: row.position,
        tags: row.tags,
        source_channel: row.source_channel,
        min_priority,
        target,
    })
}

fn target_columns(target: &RouteTarget) -> (&'static str, Option<&str>, Option<Uuid>) {
    match target {
        RouteTarget::User { user_id } => ("user", Some(user_id.as_str()), None),
        RouteTarget::TeamQueue { team_id } => ("team_queue", None, Some(*team_id)),
        RouteTarget::TeamRoundRobin { team_id } => ("team_round_robin", None, Some(*team_id)),
    }
}

impl RoutingRepo for PgRoutingRepo {
    async fn list_rules(&self, org_id: Option<i32>, domain: &str) -> Result<Vec<RoutingRule>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, org_id, domain, position, tags, source_channel,
                   min_priority, target_kind, target_user_id, target_team_id
            FROM escalation_routing_rules
            WHERE org_id IS NOT DISTINCT FROM $1 AND domain = $2
            ORDER BY position ASC, created_at ASC
            "#,
            org_id,
            domain,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                row_to_rule(RuleRow {
                    id: r.id,
                    org_id: r.org_id,
                    domain: r.domain,
                    position: r.position,
                    tags: r.tags,
                    source_channel: r.source_channel,
                    min_priority: r.min_priority,
                    target_kind: r.target_kind,
                    target_user_id: r.target_user_id,
                    target_team_id: r.target_team_id,
                })
            })
            .collect()
    }

    async fn list_all_rules(&self, org_id: Option<i32>) -> Result<Vec<RoutingRule>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, org_id, domain, position, tags, source_channel,
                   min_priority, target_kind, target_user_id, target_team_id
            FROM escalation_routing_rules
            WHERE org_id IS NOT DISTINCT FROM $1
            ORDER BY domain ASC, position ASC
            "#,
            org_id,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                row_to_rule(RuleRow {
                    id: r.id,
                    org_id: r.org_id,
                    domain: r.domain,
                    position: r.position,
                    tags: r.tags,
                    source_channel: r.source_channel,
                    min_priority: r.min_priority,
                    target_kind: r.target_kind,
                    target_user_id: r.target_user_id,
                    target_team_id: r.target_team_id,
                })
            })
            .collect()
    }

    async fn upsert_rule(&self, rule: &RoutingRule) -> Result<()> {
        let (kind, user, team) = target_columns(&rule.target);
        sqlx::query!(
            r#"
            INSERT INTO escalation_routing_rules (
                id, org_id, domain, position, tags, source_channel,
                min_priority, target_kind, target_user_id, target_team_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (id) DO UPDATE SET
                domain = EXCLUDED.domain,
                position = EXCLUDED.position,
                tags = EXCLUDED.tags,
                source_channel = EXCLUDED.source_channel,
                min_priority = EXCLUDED.min_priority,
                target_kind = EXCLUDED.target_kind,
                target_user_id = EXCLUDED.target_user_id,
                target_team_id = EXCLUDED.target_team_id
            "#,
            rule.id,
            rule.org_id,
            rule.domain,
            rule.position,
            &rule.tags,
            rule.source_channel.as_deref(),
            rule.min_priority.map(|p| p.as_str()),
            kind,
            user,
            team,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_rule(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM escalation_routing_rules WHERE id = $1", id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_expert(
        &self,
        _org_id: Option<i32>,
        user_id: &str,
    ) -> Result<Option<ExpertProfile>> {
        let row = sqlx::query!(
            r#"
            SELECT user_id, org_id, domains, tags, available
            FROM escalation_experts
            WHERE user_id = $1
            "#,
            user_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| ExpertProfile {
            user_id: r.user_id,
            org_id: r.org_id,
            domains: r.domains,
            tags: r.tags,
            available: r.available,
        }))
    }

    async fn list_experts(&self, org_id: Option<i32>) -> Result<Vec<ExpertProfile>> {
        let rows = sqlx::query!(
            r#"
            SELECT user_id, org_id, domains, tags, available
            FROM escalation_experts
            WHERE org_id IS NOT DISTINCT FROM $1
            ORDER BY user_id
            "#,
            org_id,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ExpertProfile {
                user_id: r.user_id,
                org_id: r.org_id,
                domains: r.domains,
                tags: r.tags,
                available: r.available,
            })
            .collect())
    }

    async fn upsert_expert(&self, profile: &ExpertProfile) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO escalation_experts (user_id, org_id, domains, tags, available)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id) DO UPDATE SET
                org_id = EXCLUDED.org_id,
                domains = EXCLUDED.domains,
                tags = EXCLUDED.tags,
                available = EXCLUDED.available,
                updated_at = now()
            "#,
            profile.user_id,
            profile.org_id,
            &profile.domains,
            &profile.tags,
            profile.available,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn round_robin_last(&self, rule_id: Uuid) -> Result<Option<String>> {
        let row = sqlx::query!(
            "SELECT last_user_id FROM escalation_round_robin WHERE rule_id = $1",
            rule_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.last_user_id))
    }

    async fn set_round_robin_last(&self, rule_id: Uuid, user_id: &str) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO escalation_round_robin (rule_id, last_user_id)
            VALUES ($1, $2)
            ON CONFLICT (rule_id) DO UPDATE SET
                last_user_id = EXCLUDED.last_user_id,
                updated_at = now()
            "#,
            rule_id,
            user_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
