//! Postgres implementation of the ledger storage port.

use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{Actor, ActorKind, AgentEvent, LedgerError, Result, SessionOutcome};
use crate::domain::ports::{ChainHead, EventFilter, LedgerRepo, PreparedEvent};

/// Postgres-backed ledger repo over the `agent_events` and
/// `agent_session_outcomes` tables.
#[derive(Debug, Clone)]
pub struct PgLedgerRepo {
    pool: PgPool,
}

impl PgLedgerRepo {
    /// Build a repo over the given pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct EventRow {
    session_id: Uuid,
    seq: i64,
    data: serde_json::Value,
    actor_kind: String,
    actor_id: String,
    org_id: Option<i32>,
    occurred_at: chrono::DateTime<chrono::Utc>,
    source_event_seqs: Vec<i64>,
    prev_hash: Vec<u8>,
    hash: Vec<u8>,
}

fn row_to_event(row: EventRow) -> Result<AgentEvent> {
    let payload = serde_json::from_value(row.data).map_err(LedgerError::Serialization)?;
    let kind = ActorKind::parse(&row.actor_kind).ok_or_else(|| {
        LedgerError::InvalidRequest(format!("unknown actor kind: {}", row.actor_kind))
    })?;
    Ok(AgentEvent {
        session_id: row.session_id,
        seq: row.seq,
        payload,
        actor: Actor {
            kind,
            id: row.actor_id,
        },
        org_id: row.org_id,
        occurred_at: row.occurred_at,
        source_event_seqs: row.source_event_seqs,
        prev_hash: row.prev_hash,
        hash: row.hash,
    })
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(
        err,
        sqlx::Error::Database(db) if db.constraint().is_some() && db.is_unique_violation()
    )
}

impl LedgerRepo for PgLedgerRepo {
    async fn chain_head(&self, session_id: Uuid) -> Result<Option<ChainHead>> {
        let row = sqlx::query!(
            r#"
            SELECT seq, hash
            FROM agent_events
            WHERE session_id = $1
            ORDER BY seq DESC
            LIMIT 1
            "#,
            session_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| ChainHead {
            seq: r.seq,
            hash: r.hash,
        }))
    }

    async fn insert_events(&self, events: Vec<PreparedEvent>) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for prepared in &events {
            let e = &prepared.event;
            let result = sqlx::query!(
                r#"
                INSERT INTO agent_events (
                    session_id, seq, event_type, data,
                    actor_kind, actor_id, org_id, occurred_at,
                    source_event_seqs, prev_hash, hash
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                "#,
                e.session_id,
                e.seq,
                prepared.event_type,
                prepared.payload_json,
                e.actor.kind.as_str(),
                e.actor.id,
                e.org_id,
                e.occurred_at,
                &e.source_event_seqs,
                e.prev_hash,
                e.hash,
            )
            .execute(&mut *tx)
            .await;

            if let Err(err) = result {
                if is_unique_violation(&err) {
                    return Err(LedgerError::ChainConflict {
                        expected_seq: e.seq,
                    });
                }
                return Err(err.into());
            }
        }
        tx.commit().await?;
        Ok(())
    }

    async fn list_session_events(
        &self,
        session_id: Uuid,
        from_seq: i64,
        limit: i64,
    ) -> Result<Vec<AgentEvent>> {
        let rows = sqlx::query!(
            r#"
            SELECT session_id, seq, data, actor_kind, actor_id, org_id,
                   occurred_at, source_event_seqs, prev_hash, hash
            FROM agent_events
            WHERE session_id = $1 AND seq >= $2
            ORDER BY seq ASC
            LIMIT $3
            "#,
            session_id,
            from_seq,
            limit,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| {
                row_to_event(EventRow {
                    session_id: r.session_id,
                    seq: r.seq,
                    data: r.data,
                    actor_kind: r.actor_kind,
                    actor_id: r.actor_id,
                    org_id: r.org_id,
                    occurred_at: r.occurred_at,
                    source_event_seqs: r.source_event_seqs,
                    prev_hash: r.prev_hash,
                    hash: r.hash,
                })
            })
            .collect()
    }

    async fn query_events(&self, filter: &EventFilter) -> Result<Vec<AgentEvent>> {
        let event_types: Vec<String> = filter.event_types.clone();
        let rows = sqlx::query!(
            r#"
            SELECT session_id, seq, data, actor_kind, actor_id, org_id,
                   occurred_at, source_event_seqs, prev_hash, hash
            FROM agent_events
            WHERE ($1::uuid IS NULL OR session_id = $1)
              AND ($2::int IS NULL OR org_id = $2)
              AND (cardinality($3::text[]) = 0 OR event_type = ANY($3))
              AND ($4::text IS NULL OR actor_id = $4)
              AND ($5::timestamptz IS NULL OR occurred_at >= $5)
              AND ($6::timestamptz IS NULL OR occurred_at < $6)
            ORDER BY occurred_at DESC, session_id, seq DESC
            LIMIT $7
            "#,
            filter.session_id,
            filter.org_id,
            &event_types,
            filter.actor_id.as_deref(),
            filter.occurred_after,
            filter.occurred_before,
            filter.limit,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| {
                row_to_event(EventRow {
                    session_id: r.session_id,
                    seq: r.seq,
                    data: r.data,
                    actor_kind: r.actor_kind,
                    actor_id: r.actor_id,
                    org_id: r.org_id,
                    occurred_at: r.occurred_at,
                    source_event_seqs: r.source_event_seqs,
                    prev_hash: r.prev_hash,
                    hash: r.hash,
                })
            })
            .collect()
    }

    async fn upsert_outcome(
        &self,
        session_id: Uuid,
        outcome: &SessionOutcome,
        summary: Option<&str>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO agent_session_outcomes (session_id, outcome, summary)
            VALUES ($1, $2, $3)
            ON CONFLICT (session_id) DO UPDATE
            SET outcome = EXCLUDED.outcome,
                summary = EXCLUDED.summary,
                updated_at = NOW()
            "#,
            session_id,
            outcome.as_str(),
            summary,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
