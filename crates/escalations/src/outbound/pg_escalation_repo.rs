//! Postgres implementation of the escalation storage port.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{
    Escalation, EscalationError, EscalationStatus, EscalationTransition, Priority, Result,
};
use crate::domain::ports::{EscalationFilter, EscalationRepo};

/// Postgres-backed escalation repo over the `escalations` tables.
#[derive(Debug, Clone)]
pub struct PgEscalationRepo {
    pool: PgPool,
}

impl PgEscalationRepo {
    /// Build a repo over the given pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct EscalationRow {
    id: Uuid,
    org_id: Option<i32>,
    domain: String,
    session_id: Option<Uuid>,
    requester_user_id: Option<String>,
    requester_display: String,
    source_channel: Option<String>,
    title: String,
    summary: String,
    tags: Vec<String>,
    priority: String,
    status: String,
    assignee_user_id: Option<String>,
    assignee_team_id: Option<Uuid>,
    callback_url: Option<String>,
    resolution: Option<String>,
    resolved_by: Option<String>,
    created_at: DateTime<Utc>,
    claimed_at: Option<DateTime<Utc>>,
    resolved_at: Option<DateTime<Utc>>,
}

fn row_to_escalation(row: EscalationRow) -> Result<Escalation> {
    let priority = Priority::parse(&row.priority).ok_or_else(|| {
        EscalationError::InvalidRequest(format!("unknown priority: {}", row.priority))
    })?;
    let status = EscalationStatus::parse(&row.status).ok_or_else(|| {
        EscalationError::InvalidRequest(format!("unknown status: {}", row.status))
    })?;
    Ok(Escalation {
        id: row.id,
        org_id: row.org_id,
        domain: row.domain,
        session_id: row.session_id,
        requester_user_id: row.requester_user_id,
        requester_display: row.requester_display,
        source_channel: row.source_channel,
        title: row.title,
        summary: row.summary,
        tags: row.tags,
        priority,
        status,
        assignee_user_id: row.assignee_user_id,
        assignee_team_id: row.assignee_team_id,
        callback_url: row.callback_url,
        resolution: row.resolution,
        resolved_by: row.resolved_by,
        created_at: row.created_at,
        claimed_at: row.claimed_at,
        resolved_at: row.resolved_at,
    })
}

macro_rules! escalation_from_record {
    ($r:expr) => {
        row_to_escalation(EscalationRow {
            id: $r.id,
            org_id: $r.org_id,
            domain: $r.domain,
            session_id: $r.session_id,
            requester_user_id: $r.requester_user_id,
            requester_display: $r.requester_display,
            source_channel: $r.source_channel,
            title: $r.title,
            summary: $r.summary,
            tags: $r.tags,
            priority: $r.priority,
            status: $r.status,
            assignee_user_id: $r.assignee_user_id,
            assignee_team_id: $r.assignee_team_id,
            callback_url: $r.callback_url,
            resolution: $r.resolution,
            resolved_by: $r.resolved_by,
            created_at: $r.created_at,
            claimed_at: $r.claimed_at,
            resolved_at: $r.resolved_at,
        })
    };
}

const SELECT_COLUMNS: &str = r#"
    id, org_id, domain, session_id, requester_user_id, requester_display,
    source_channel, title, summary, tags, priority, status,
    assignee_user_id, assignee_team_id, callback_url, resolution,
    resolved_by, created_at, claimed_at, resolved_at
"#;

impl EscalationRepo for PgEscalationRepo {
    async fn insert(&self, e: &Escalation) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO escalations (
                id, org_id, domain, session_id, requester_user_id,
                requester_display, source_channel, title, summary, tags,
                priority, status, assignee_user_id, assignee_team_id,
                callback_url, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                    $13, $14, $15, $16)
            "#,
            e.id,
            e.org_id,
            e.domain,
            e.session_id,
            e.requester_user_id.as_deref(),
            e.requester_display,
            e.source_channel.as_deref(),
            e.title,
            e.summary,
            &e.tags,
            e.priority.as_str(),
            e.status.as_str(),
            e.assignee_user_id.as_deref(),
            e.assignee_team_id,
            e.callback_url.as_deref(),
            e.created_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<Escalation>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, domain, session_id, requester_user_id,
                   requester_display, source_channel, title, summary, tags,
                   priority, status, assignee_user_id, assignee_team_id,
                   callback_url, resolution, resolved_by, created_at,
                   claimed_at, resolved_at
            FROM escalations
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| escalation_from_record!(r)).transpose()
    }

    async fn list(&self, filter: &EscalationFilter) -> Result<Vec<Escalation>> {
        // Dynamic filter combination over a fixed column set; the
        // conditions are all optional, so a dynamic query builder is the
        // pragmatic choice here.
        let mut builder = sqlx::QueryBuilder::new(format!(
            "SELECT {SELECT_COLUMNS} FROM escalations WHERE TRUE"
        ));
        if let Some(org_id) = filter.org_id {
            builder.push(" AND org_id = ").push_bind(org_id);
        }
        if let Some(status) = filter.status {
            builder.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(domain) = &filter.domain {
            builder.push(" AND domain = ").push_bind(domain.clone());
        }
        if let Some(user) = &filter.assignee_user_id {
            builder
                .push(" AND assignee_user_id = ")
                .push_bind(user.clone());
        }
        if let Some(team) = filter.assignee_team_id {
            builder.push(" AND assignee_team_id = ").push_bind(team);
        }
        if filter.unclaimed_only {
            builder.push(" AND assignee_user_id IS NULL");
        }
        builder
            .push(" ORDER BY created_at DESC LIMIT ")
            .push_bind(if filter.limit > 0 { filter.limit } else { 100 });

        let rows = builder.build().fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|row| {
                use sqlx::Row;
                row_to_escalation(EscalationRow {
                    id: row.try_get("id")?,
                    org_id: row.try_get("org_id")?,
                    domain: row.try_get("domain")?,
                    session_id: row.try_get("session_id")?,
                    requester_user_id: row.try_get("requester_user_id")?,
                    requester_display: row.try_get("requester_display")?,
                    source_channel: row.try_get("source_channel")?,
                    title: row.try_get("title")?,
                    summary: row.try_get("summary")?,
                    tags: row.try_get("tags")?,
                    priority: row.try_get("priority")?,
                    status: row.try_get("status")?,
                    assignee_user_id: row.try_get("assignee_user_id")?,
                    assignee_team_id: row.try_get("assignee_team_id")?,
                    callback_url: row.try_get("callback_url")?,
                    resolution: row.try_get("resolution")?,
                    resolved_by: row.try_get("resolved_by")?,
                    created_at: row.try_get("created_at")?,
                    claimed_at: row.try_get("claimed_at")?,
                    resolved_at: row.try_get("resolved_at")?,
                })
            })
            .collect()
    }

    async fn claim(&self, id: Uuid, user_id: &str) -> Result<Option<Escalation>> {
        let row = sqlx::query!(
            r#"
            UPDATE escalations
            SET status = 'claimed', assignee_user_id = $2, claimed_at = now()
            WHERE id = $1 AND status = 'open'
            RETURNING id, org_id, domain, session_id, requester_user_id,
                      requester_display, source_channel, title, summary,
                      tags, priority, status, assignee_user_id,
                      assignee_team_id, callback_url, resolution,
                      resolved_by, created_at, claimed_at, resolved_at
            "#,
            id,
            user_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| escalation_from_record!(r)).transpose()
    }

    async fn reassign(
        &self,
        id: Uuid,
        to_user: Option<&str>,
        to_team: Option<Uuid>,
    ) -> Result<Option<Escalation>> {
        let row = sqlx::query!(
            r#"
            UPDATE escalations
            SET assignee_user_id = $2,
                assignee_team_id = COALESCE($3, assignee_team_id),
                status = CASE WHEN $2::TEXT IS NULL THEN 'open' ELSE 'claimed' END,
                claimed_at = CASE WHEN $2::TEXT IS NULL THEN NULL ELSE now() END
            WHERE id = $1 AND status IN ('open', 'claimed')
            RETURNING id, org_id, domain, session_id, requester_user_id,
                      requester_display, source_channel, title, summary,
                      tags, priority, status, assignee_user_id,
                      assignee_team_id, callback_url, resolution,
                      resolved_by, created_at, claimed_at, resolved_at
            "#,
            id,
            to_user,
            to_team,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| escalation_from_record!(r)).transpose()
    }

    async fn resolve(
        &self,
        id: Uuid,
        resolution: &str,
        resolved_by: &str,
    ) -> Result<Option<Escalation>> {
        let row = sqlx::query!(
            r#"
            UPDATE escalations
            SET status = 'resolved', resolution = $2, resolved_by = $3,
                resolved_at = now()
            WHERE id = $1 AND status IN ('open', 'claimed')
            RETURNING id, org_id, domain, session_id, requester_user_id,
                      requester_display, source_channel, title, summary,
                      tags, priority, status, assignee_user_id,
                      assignee_team_id, callback_url, resolution,
                      resolved_by, created_at, claimed_at, resolved_at
            "#,
            id,
            resolution,
            resolved_by,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| escalation_from_record!(r)).transpose()
    }

    async fn cancel(&self, id: Uuid) -> Result<Option<Escalation>> {
        let row = sqlx::query!(
            r#"
            UPDATE escalations
            SET status = 'cancelled', resolved_at = now()
            WHERE id = $1 AND status IN ('open', 'claimed')
            RETURNING id, org_id, domain, session_id, requester_user_id,
                      requester_display, source_channel, title, summary,
                      tags, priority, status, assignee_user_id,
                      assignee_team_id, callback_url, resolution,
                      resolved_by, created_at, claimed_at, resolved_at
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| escalation_from_record!(r)).transpose()
    }

    async fn insert_transition(&self, t: &EscalationTransition) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO escalation_transitions (
                id, escalation_id, action, actor_id, from_user_id,
                from_team_id, to_user_id, to_team_id, reason, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
            t.id,
            t.escalation_id,
            t.action,
            t.actor_id,
            t.from_user_id.as_deref(),
            t.from_team_id,
            t.to_user_id.as_deref(),
            t.to_team_id,
            t.reason.as_deref(),
            t.created_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_transitions(&self, escalation_id: Uuid) -> Result<Vec<EscalationTransition>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, escalation_id, action, actor_id, from_user_id,
                   from_team_id, to_user_id, to_team_id, reason, created_at
            FROM escalation_transitions
            WHERE escalation_id = $1
            ORDER BY created_at ASC
            "#,
            escalation_id,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| EscalationTransition {
                id: r.id,
                escalation_id: r.escalation_id,
                action: r.action,
                actor_id: r.actor_id,
                from_user_id: r.from_user_id,
                from_team_id: r.from_team_id,
                to_user_id: r.to_user_id,
                to_team_id: r.to_team_id,
                reason: r.reason,
                created_at: r.created_at,
            })
            .collect())
    }
}
