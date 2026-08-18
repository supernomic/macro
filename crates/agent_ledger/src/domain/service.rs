//! Ledger domain service: hash-chained appends, replay, verification.

#[cfg(test)]
mod test;

use macro_uuid::Uuid;

use super::model::{
    Actor, AgentEvent, GENESIS_HASH, LedgerError, NewAgentEvent, Result, SessionOutcome, chain_hash,
};
use super::ports::{ChainHead, EventFilter, LedgerRepo, LedgerService, PreparedEvent};

/// How many times an append is retried after a chain conflict before giving
/// up. Conflicts only happen when two writers race on the same session.
const MAX_CHAIN_RETRIES: usize = 3;

/// Default and maximum page sizes for queries.
pub const DEFAULT_QUERY_LIMIT: i64 = 200;
/// Hard cap applied to caller-provided limits.
pub const MAX_QUERY_LIMIT: i64 = 2_000;

/// Concrete ledger service over a [`LedgerRepo`].
#[derive(Debug, Clone)]
pub struct LedgerServiceImpl<R> {
    repo: R,
}

impl<R> LedgerServiceImpl<R> {
    /// Build a service over the given repo.
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
}

fn prepare_batch(
    session_id: Uuid,
    org_id: Option<i32>,
    head: Option<&ChainHead>,
    events: &[NewAgentEvent],
) -> Result<Vec<PreparedEvent>> {
    let (mut seq, mut prev_hash) = match head {
        Some(h) => (h.seq + 1, h.hash.clone()),
        None => (0, GENESIS_HASH.to_vec()),
    };

    let mut prepared = Vec::with_capacity(events.len());
    for event in events {
        let payload_json =
            serde_json::to_value(&event.payload).map_err(LedgerError::Serialization)?;
        let payload_bytes =
            serde_json::to_vec(&payload_json).map_err(LedgerError::Serialization)?;
        let hash = chain_hash(
            &prev_hash,
            session_id,
            seq,
            event.occurred_at,
            &event.actor,
            &payload_bytes,
        );

        prepared.push(PreparedEvent {
            event: AgentEvent {
                session_id,
                seq,
                payload: event.payload.clone(),
                actor: event.actor.clone(),
                org_id,
                occurred_at: event.occurred_at,
                source_event_seqs: event.source_event_seqs.clone(),
                prev_hash: prev_hash.clone(),
                hash: hash.clone(),
            },
            payload_json,
            event_type: event.payload.event_type(),
        });

        prev_hash = hash;
        seq += 1;
    }

    Ok(prepared)
}

impl<R: LedgerRepo> LedgerService for LedgerServiceImpl<R> {
    #[tracing::instrument(skip(self, events), fields(session_id = %session_id, n = events.len()), err)]
    async fn append_events(
        &self,
        session_id: Uuid,
        org_id: Option<i32>,
        events: Vec<NewAgentEvent>,
    ) -> Result<Vec<AgentEvent>> {
        if events.is_empty() {
            return Ok(Vec::new());
        }

        let mut attempt = 0;
        loop {
            let head = self.repo.chain_head(session_id).await?;
            let prepared = prepare_batch(session_id, org_id, head.as_ref(), &events)?;
            let stored: Vec<AgentEvent> = prepared.iter().map(|p| p.event.clone()).collect();

            match self.repo.insert_events(prepared).await {
                Ok(()) => return Ok(stored),
                Err(LedgerError::ChainConflict { .. }) if attempt < MAX_CHAIN_RETRIES => {
                    attempt += 1;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    #[tracing::instrument(skip(self), err)]
    async fn list_session_events(
        &self,
        session_id: Uuid,
        from_seq: i64,
        limit: i64,
    ) -> Result<Vec<AgentEvent>> {
        let limit = limit.clamp(1, MAX_QUERY_LIMIT);
        self.repo
            .list_session_events(session_id, from_seq.max(0), limit)
            .await
    }

    #[tracing::instrument(skip(self, filter), err)]
    async fn query_events(&self, mut filter: EventFilter) -> Result<Vec<AgentEvent>> {
        if filter.limit <= 0 {
            filter.limit = DEFAULT_QUERY_LIMIT;
        }
        filter.limit = filter.limit.min(MAX_QUERY_LIMIT);
        self.repo.query_events(&filter).await
    }

    #[tracing::instrument(skip(self), err)]
    async fn verify_chain(&self, session_id: Uuid) -> Result<Option<i64>> {
        let mut expected_prev: Vec<u8> = GENESIS_HASH.to_vec();
        let mut expected_seq: i64 = 0;
        let mut from_seq: i64 = 0;

        loop {
            let page = self
                .repo
                .list_session_events(session_id, from_seq, MAX_QUERY_LIMIT)
                .await?;
            if page.is_empty() {
                return Ok(None);
            }

            for event in &page {
                if event.seq != expected_seq || event.prev_hash != expected_prev {
                    return Ok(Some(event.seq));
                }
                let payload_json =
                    serde_json::to_value(&event.payload).map_err(LedgerError::Serialization)?;
                let payload_bytes =
                    serde_json::to_vec(&payload_json).map_err(LedgerError::Serialization)?;
                let recomputed = chain_hash(
                    &event.prev_hash,
                    session_id,
                    event.seq,
                    event.occurred_at,
                    &event.actor,
                    &payload_bytes,
                );
                if recomputed != event.hash {
                    return Ok(Some(event.seq));
                }
                expected_prev = event.hash.clone();
                expected_seq = event.seq + 1;
            }

            from_seq = expected_seq;
            if (page.len() as i64) < MAX_QUERY_LIMIT {
                return Ok(None);
            }
        }
    }

    #[tracing::instrument(skip(self, summary), err)]
    async fn record_outcome(
        &self,
        session_id: Uuid,
        outcome: SessionOutcome,
        summary: Option<String>,
        _actor: Actor,
    ) -> Result<()> {
        // The outcome row is a queryable projection; the authoritative record
        // is the `turn/end` event stream, which callers append separately.
        self.repo
            .upsert_outcome(session_id, &outcome, summary.as_deref())
            .await
    }
}
