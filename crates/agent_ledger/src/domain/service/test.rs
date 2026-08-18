use std::sync::Mutex;

use chrono::Utc;
use macro_uuid::Uuid;

use super::*;
use crate::domain::model::{Actor, ActorKind, AgentEventPayload, UserMessageSource};
use crate::domain::ports::EventFilter;

/// In-memory fake repo good enough to exercise chaining, retry, and
/// verification logic.
#[derive(Default)]
struct FakeRepo {
    events: Mutex<Vec<PreparedEvent>>,
    /// When set, the next N insert attempts fail with a chain conflict.
    conflicts_remaining: Mutex<usize>,
}

impl LedgerRepo for FakeRepo {
    async fn chain_head(&self, session_id: Uuid) -> Result<Option<ChainHead>> {
        let events = self.events.lock().unwrap();
        Ok(events
            .iter()
            .filter(|p| p.event.session_id == session_id)
            .max_by_key(|p| p.event.seq)
            .map(|p| ChainHead {
                seq: p.event.seq,
                hash: p.event.hash.clone(),
            }))
    }

    async fn insert_events(&self, prepared: Vec<PreparedEvent>) -> Result<()> {
        {
            let mut conflicts = self.conflicts_remaining.lock().unwrap();
            if *conflicts > 0 {
                *conflicts -= 1;
                return Err(LedgerError::ChainConflict {
                    expected_seq: prepared[0].event.seq,
                });
            }
        }
        let mut events = self.events.lock().unwrap();
        for p in &prepared {
            if events
                .iter()
                .any(|e| e.event.session_id == p.event.session_id && e.event.seq == p.event.seq)
            {
                return Err(LedgerError::ChainConflict {
                    expected_seq: p.event.seq,
                });
            }
        }
        events.extend(prepared);
        Ok(())
    }

    async fn list_session_events(
        &self,
        session_id: Uuid,
        from_seq: i64,
        limit: i64,
    ) -> Result<Vec<AgentEvent>> {
        let events = self.events.lock().unwrap();
        let mut out: Vec<AgentEvent> = events
            .iter()
            .filter(|p| p.event.session_id == session_id && p.event.seq >= from_seq)
            .map(|p| p.event.clone())
            .collect();
        out.sort_by_key(|e| e.seq);
        out.truncate(limit as usize);
        Ok(out)
    }

    async fn query_events(&self, filter: &EventFilter) -> Result<Vec<AgentEvent>> {
        let events = self.events.lock().unwrap();
        let mut out: Vec<AgentEvent> = events
            .iter()
            .filter(|p| {
                filter
                    .session_id
                    .is_none_or(|sid| p.event.session_id == sid)
                    && (filter.event_types.is_empty()
                        || filter.event_types.iter().any(|t| t == p.event_type))
            })
            .map(|p| p.event.clone())
            .collect();
        out.truncate(filter.limit as usize);
        Ok(out)
    }

    async fn upsert_outcome(
        &self,
        _session_id: Uuid,
        _outcome: &SessionOutcome,
        _summary: Option<&str>,
    ) -> Result<()> {
        Ok(())
    }
}

fn user_message(content: &str) -> NewAgentEvent {
    NewAgentEvent {
        payload: AgentEventPayload::UserMessage {
            content: content.to_string(),
            source: UserMessageSource::Human,
        },
        actor: Actor {
            kind: ActorKind::User,
            id: "user-1".to_string(),
        },
        occurred_at: Utc::now(),
        source_event_seqs: Vec::new(),
    }
}

#[tokio::test]
async fn append_assigns_contiguous_seqs_and_chains_hashes() {
    let service = LedgerServiceImpl::new(FakeRepo::default());
    let session = macro_uuid::generate_uuid_v7();

    let first = service
        .append_events(session, Some(1), vec![user_message("a"), user_message("b")])
        .await
        .unwrap();
    let second = service
        .append_events(session, Some(1), vec![user_message("c")])
        .await
        .unwrap();

    assert_eq!(first[0].seq, 0);
    assert_eq!(first[1].seq, 1);
    assert_eq!(second[0].seq, 2);
    assert_eq!(first[0].prev_hash, GENESIS_HASH.to_vec());
    assert_eq!(first[1].prev_hash, first[0].hash);
    assert_eq!(second[0].prev_hash, first[1].hash);
}

#[tokio::test]
async fn append_retries_after_chain_conflict() {
    let repo = FakeRepo::default();
    *repo.conflicts_remaining.lock().unwrap() = 2;
    let service = LedgerServiceImpl::new(repo);
    let session = macro_uuid::generate_uuid_v7();

    let stored = service
        .append_events(session, None, vec![user_message("a")])
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
}

#[tokio::test]
async fn verify_chain_detects_tampering() {
    let repo = FakeRepo::default();
    let service = LedgerServiceImpl::new(repo);
    let session = macro_uuid::generate_uuid_v7();

    service
        .append_events(
            session,
            None,
            vec![user_message("a"), user_message("b"), user_message("c")],
        )
        .await
        .unwrap();

    assert_eq!(service.verify_chain(session).await.unwrap(), None);

    // Tamper with the stored payload of seq 1.
    {
        let mut events = service.repo.events.lock().unwrap();
        let target = events
            .iter_mut()
            .find(|p| p.event.session_id == session && p.event.seq == 1)
            .unwrap();
        target.event.payload = AgentEventPayload::UserMessage {
            content: "tampered".to_string(),
            source: UserMessageSource::Human,
        };
    }

    assert_eq!(service.verify_chain(session).await.unwrap(), Some(1));
}

#[tokio::test]
async fn empty_append_is_a_noop() {
    let service = LedgerServiceImpl::new(FakeRepo::default());
    let session = macro_uuid::generate_uuid_v7();
    let stored = service.append_events(session, None, vec![]).await.unwrap();
    assert!(stored.is_empty());
}
