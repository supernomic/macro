//! Postgres implementation of the approval-request storage port.

use chrono::{DateTime, Utc};
use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{
    ApprovalError, ApprovalRequest, ApprovalStatus, ApprovalTransition, Result,
};
use crate::domain::ports::{ApprovalFilter, ApprovalRepo};

/// Postgres-backed approval repo over the `approval_requests` tables.
#[derive(Debug, Clone)]
pub struct PgApprovalRepo {
    pool: PgPool,
}

impl PgApprovalRepo {
    /// Build a repo over the given pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct ApprovalRow {
    id: Uuid,
    org_id: Option<i32>,
    agent_slug: String,
    session_id: Option<Uuid>,
    requester_user_id: Option<String>,
    requester_display: String,
    tool_name: String,
    arguments: serde_json::Value,
    arguments_digest: String,
    summary: String,
    status: String,
    assignee_user_id: Option<String>,
    assignee_team_id: Option<Uuid>,
    callback_url: Option<String>,
    decided_by: Option<String>,
    decision_note: Option<String>,
    created_at: DateTime<Utc>,
    decided_at: Option<DateTime<Utc>>,
    consumed_at: Option<DateTime<Utc>>,
}

fn row_to_request(row: ApprovalRow) -> Result<ApprovalRequest> {
    let status = ApprovalStatus::parse(&row.status)
        .ok_or_else(|| ApprovalError::InvalidRequest(format!("unknown status: {}", row.status)))?;
    Ok(ApprovalRequest {
        id: row.id,
        org_id: row.org_id,
        agent_slug: row.agent_slug,
        session_id: row.session_id,
        requester_user_id: row.requester_user_id,
        requester_display: row.requester_display,
        tool_name: row.tool_name,
        arguments: row.arguments,
        arguments_digest: row.arguments_digest,
        summary: row.summary,
        status,
        assignee_user_id: row.assignee_user_id,
        assignee_team_id: row.assignee_team_id,
        callback_url: row.callback_url,
        decided_by: row.decided_by,
        decision_note: row.decision_note,
        created_at: row.created_at,
        decided_at: row.decided_at,
        consumed_at: row.consumed_at,
    })
}

macro_rules! request_from_record {
    ($r:expr) => {
        row_to_request(ApprovalRow {
            id: $r.id,
            org_id: $r.org_id,
            agent_slug: $r.agent_slug,
            session_id: $r.session_id,
            requester_user_id: $r.requester_user_id,
            requester_display: $r.requester_display,
            tool_name: $r.tool_name,
            arguments: $r.arguments,
            arguments_digest: $r.arguments_digest,
            summary: $r.summary,
            status: $r.status,
            assignee_user_id: $r.assignee_user_id,
            assignee_team_id: $r.assignee_team_id,
            callback_url: $r.callback_url,
            decided_by: $r.decided_by,
            decision_note: $r.decision_note,
            created_at: $r.created_at,
            decided_at: $r.decided_at,
            consumed_at: $r.consumed_at,
        })
    };
}

const SELECT_COLUMNS: &str = r#"
    id, org_id, agent_slug, session_id, requester_user_id,
    requester_display, tool_name, arguments, arguments_digest, summary,
    status, assignee_user_id, assignee_team_id, callback_url, decided_by,
    decision_note, created_at, decided_at, consumed_at
"#;

impl ApprovalRepo for PgApprovalRepo {
    async fn insert(&self, r: &ApprovalRequest) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO approval_requests (
                id, org_id, agent_slug, session_id, requester_user_id,
                requester_display, tool_name, arguments, arguments_digest,
                summary, status, assignee_user_id, assignee_team_id,
                callback_url, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                    $13, $14, $15)
            "#,
            r.id,
            r.org_id,
            r.agent_slug,
            r.session_id,
            r.requester_user_id.as_deref(),
            r.requester_display,
            r.tool_name,
            &r.arguments,
            r.arguments_digest,
            r.summary,
            r.status.as_str(),
            r.assignee_user_id.as_deref(),
            r.assignee_team_id,
            r.callback_url.as_deref(),
            r.created_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<ApprovalRequest>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, agent_slug, session_id, requester_user_id,
                   requester_display, tool_name, arguments,
                   arguments_digest, summary, status, assignee_user_id,
                   assignee_team_id, callback_url, decided_by,
                   decision_note, created_at, decided_at, consumed_at
            FROM approval_requests
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| request_from_record!(r)).transpose()
    }

    async fn list(&self, filter: &ApprovalFilter) -> Result<Vec<ApprovalRequest>> {
        // Dynamic filter combination over a fixed column set.
        let mut builder = sqlx::QueryBuilder::new(format!(
            "SELECT {SELECT_COLUMNS} FROM approval_requests WHERE TRUE"
        ));
        if let Some(org_id) = filter.org_id {
            builder.push(" AND org_id = ").push_bind(org_id);
        }
        if let Some(status) = filter.status {
            builder.push(" AND status = ").push_bind(status.as_str());
        }
        if let Some(user) = &filter.assignee_user_id {
            builder
                .push(" AND assignee_user_id = ")
                .push_bind(user.clone());
        }
        if let Some(team) = filter.assignee_team_id {
            builder.push(" AND assignee_team_id = ").push_bind(team);
        }
        if filter.unassigned_only {
            builder.push(" AND assignee_user_id IS NULL");
        }
        builder
            .push(" ORDER BY created_at DESC LIMIT ")
            .push_bind(if filter.limit > 0 { filter.limit } else { 100 });

        let rows = builder.build().fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|row| {
                use sqlx::Row;
                row_to_request(ApprovalRow {
                    id: row.try_get("id")?,
                    org_id: row.try_get("org_id")?,
                    agent_slug: row.try_get("agent_slug")?,
                    session_id: row.try_get("session_id")?,
                    requester_user_id: row.try_get("requester_user_id")?,
                    requester_display: row.try_get("requester_display")?,
                    tool_name: row.try_get("tool_name")?,
                    arguments: row.try_get("arguments")?,
                    arguments_digest: row.try_get("arguments_digest")?,
                    summary: row.try_get("summary")?,
                    status: row.try_get("status")?,
                    assignee_user_id: row.try_get("assignee_user_id")?,
                    assignee_team_id: row.try_get("assignee_team_id")?,
                    callback_url: row.try_get("callback_url")?,
                    decided_by: row.try_get("decided_by")?,
                    decision_note: row.try_get("decision_note")?,
                    created_at: row.try_get("created_at")?,
                    decided_at: row.try_get("decided_at")?,
                    consumed_at: row.try_get("consumed_at")?,
                })
            })
            .collect()
    }

    async fn find_latest_for_gate(
        &self,
        session_id: Option<Uuid>,
        tool_name: &str,
        arguments_digest: &str,
    ) -> Result<Option<ApprovalRequest>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, agent_slug, session_id, requester_user_id,
                   requester_display, tool_name, arguments,
                   arguments_digest, summary, status, assignee_user_id,
                   assignee_team_id, callback_url, decided_by,
                   decision_note, created_at, decided_at, consumed_at
            FROM approval_requests
            WHERE session_id IS NOT DISTINCT FROM $1
              AND tool_name = $2
              AND arguments_digest = $3
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            session_id,
            tool_name,
            arguments_digest,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| request_from_record!(r)).transpose()
    }

    async fn decide(
        &self,
        id: Uuid,
        approved: bool,
        decided_by: &str,
        note: Option<&str>,
    ) -> Result<Option<ApprovalRequest>> {
        let row = sqlx::query!(
            r#"
            UPDATE approval_requests
            SET status = CASE WHEN $2 THEN 'approved' ELSE 'denied' END,
                decided_by = $3, decision_note = $4, decided_at = now()
            WHERE id = $1 AND status = 'pending'
            RETURNING id, org_id, agent_slug, session_id,
                      requester_user_id, requester_display, tool_name,
                      arguments, arguments_digest, summary, status,
                      assignee_user_id, assignee_team_id, callback_url,
                      decided_by, decision_note, created_at, decided_at,
                      consumed_at
            "#,
            id,
            approved,
            decided_by,
            note,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| request_from_record!(r)).transpose()
    }

    async fn consume(&self, id: Uuid) -> Result<Option<ApprovalRequest>> {
        let row = sqlx::query!(
            r#"
            UPDATE approval_requests
            SET consumed_at = now()
            WHERE id = $1
              AND status IN ('approved', 'denied')
              AND consumed_at IS NULL
            RETURNING id, org_id, agent_slug, session_id,
                      requester_user_id, requester_display, tool_name,
                      arguments, arguments_digest, summary, status,
                      assignee_user_id, assignee_team_id, callback_url,
                      decided_by, decision_note, created_at, decided_at,
                      consumed_at
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| request_from_record!(r)).transpose()
    }

    async fn reassign(
        &self,
        id: Uuid,
        to_user: Option<&str>,
        to_team: Option<Uuid>,
    ) -> Result<Option<ApprovalRequest>> {
        let row = sqlx::query!(
            r#"
            UPDATE approval_requests
            SET assignee_user_id = $2,
                assignee_team_id = COALESCE($3, assignee_team_id)
            WHERE id = $1 AND status = 'pending'
            RETURNING id, org_id, agent_slug, session_id,
                      requester_user_id, requester_display, tool_name,
                      arguments, arguments_digest, summary, status,
                      assignee_user_id, assignee_team_id, callback_url,
                      decided_by, decision_note, created_at, decided_at,
                      consumed_at
            "#,
            id,
            to_user,
            to_team,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| request_from_record!(r)).transpose()
    }

    async fn cancel(&self, id: Uuid) -> Result<Option<ApprovalRequest>> {
        let row = sqlx::query!(
            r#"
            UPDATE approval_requests
            SET status = 'cancelled', decided_at = now()
            WHERE id = $1 AND status = 'pending'
            RETURNING id, org_id, agent_slug, session_id,
                      requester_user_id, requester_display, tool_name,
                      arguments, arguments_digest, summary, status,
                      assignee_user_id, assignee_team_id, callback_url,
                      decided_by, decision_note, created_at, decided_at,
                      consumed_at
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| request_from_record!(r)).transpose()
    }

    async fn insert_transition(&self, t: &ApprovalTransition) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO approval_transitions (
                id, approval_id, action, actor_id, from_user_id,
                from_team_id, to_user_id, to_team_id, reason, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
            t.id,
            t.approval_id,
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

    async fn list_transitions(&self, approval_id: Uuid) -> Result<Vec<ApprovalTransition>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, approval_id, action, actor_id, from_user_id,
                   from_team_id, to_user_id, to_team_id, reason, created_at
            FROM approval_transitions
            WHERE approval_id = $1
            ORDER BY created_at ASC
            "#,
            approval_id,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ApprovalTransition {
                id: r.id,
                approval_id: r.approval_id,
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
