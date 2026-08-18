use std::sync::Mutex;

use agent_identity::domain::model::{AgentKind, AgentPrincipal, VerifiedAgent};
use chrono::Utc;

use super::*;
use crate::domain::model::{Actor, ActorKind, AgentEventPayload, UserMessageSource};
use crate::domain::ports::EventFilter;

#[derive(Default)]
struct FakeLedger {
    appended: Mutex<Vec<(Uuid, Option<i32>, usize)>>,
}

impl LedgerService for FakeLedger {
    async fn append_events(
        &self,
        session_id: Uuid,
        org_id: Option<i32>,
        events: Vec<NewAgentEvent>,
    ) -> Result<Vec<AgentEvent>> {
        self.appended
            .lock()
            .unwrap()
            .push((session_id, org_id, events.len()));
        Ok(Vec::new())
    }

    async fn list_session_events(
        &self,
        _session_id: Uuid,
        _from_seq: i64,
        _limit: i64,
    ) -> Result<Vec<AgentEvent>> {
        Ok(Vec::new())
    }

    async fn query_events(&self, _filter: EventFilter) -> Result<Vec<AgentEvent>> {
        Ok(Vec::new())
    }

    async fn verify_chain(&self, _session_id: Uuid) -> Result<Option<i64>> {
        Ok(None)
    }

    async fn record_outcome(
        &self,
        _session_id: Uuid,
        _outcome: SessionOutcome,
        _summary: Option<String>,
        _actor: Actor,
    ) -> Result<()> {
        Ok(())
    }
}

#[derive(Default)]
struct FakeMappings {
    mappings: Mutex<Vec<SessionMapping>>,
}

impl SessionMappingRepo for FakeMappings {
    async fn create_mapping(&self, mapping: &SessionMapping) -> Result<()> {
        self.mappings.lock().unwrap().push(mapping.clone());
        Ok(())
    }

    async fn find_by_runtime_conversation(
        &self,
        runtime_conversation_id: &str,
    ) -> Result<Option<SessionMapping>> {
        Ok(self
            .mappings
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.runtime_conversation_id == runtime_conversation_id)
            .cloned())
    }

    async fn find_by_external_thread(
        &self,
        kind: &ExternalThreadKind,
        key: &str,
    ) -> Result<Option<SessionMapping>> {
        Ok(self
            .mappings
            .lock()
            .unwrap()
            .iter()
            .find(|m| {
                m.external_thread_kind.as_ref() == Some(kind)
                    && m.external_thread_key.as_deref() == Some(key)
            })
            .cloned())
    }

    async fn find_by_session(&self, session_id: Uuid) -> Result<Option<SessionMapping>> {
        Ok(self
            .mappings
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.session_id == session_id)
            .cloned())
    }
}

fn agent(org_id: Option<i32>, scopes: &[&str]) -> VerifiedAgent {
    VerifiedAgent {
        principal: AgentPrincipal {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            slug: "techops".to_string(),
            display_name: "TechOps".to_string(),
            kind: AgentKind::DomainAgent,
            created_at: Utc::now(),
            disabled_at: None,
        },
        token_id: macro_uuid::generate_uuid_v7(),
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
    }
}

fn message() -> NewAgentEvent {
    NewAgentEvent {
        payload: AgentEventPayload::UserMessage {
            content: "hi".to_string(),
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
async fn open_session_is_idempotent_by_runtime_conversation() {
    let facade = AgentLedgerFacade::new(FakeLedger::default(), FakeMappings::default());
    let agent = agent(Some(1), &["ledger:append"]);

    let a = facade
        .open_session(
            &agent,
            OpenSession {
                runtime_conversation_id: "conv-1".to_string(),
                external_thread_kind: Some(ExternalThreadKind::SlackThread),
                external_thread_key: Some("C1:171.1".to_string()),
            },
        )
        .await
        .unwrap();
    let b = facade
        .open_session(
            &agent,
            OpenSession {
                runtime_conversation_id: "conv-1".to_string(),
                external_thread_kind: None,
                external_thread_key: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(a.session_id, b.session_id);
    assert_eq!(a.org_id, Some(1));
}

#[tokio::test]
async fn append_requires_scope_and_org_ownership() {
    let facade = AgentLedgerFacade::new(FakeLedger::default(), FakeMappings::default());
    let owner = agent(Some(1), &["ledger:append", "ledger:query"]);
    let outsider = agent(Some(2), &["ledger:append", "ledger:query"]);
    let unscoped = agent(Some(1), &["tool:search"]);

    let session = facade
        .open_session(
            &owner,
            OpenSession {
                runtime_conversation_id: "conv-1".to_string(),
                external_thread_kind: None,
                external_thread_key: None,
            },
        )
        .await
        .unwrap();

    // Owner can append; org_id is stamped from the mapping.
    facade
        .append_events(&owner, session.session_id, vec![message()])
        .await
        .unwrap();
    assert_eq!(
        facade.ledger.appended.lock().unwrap()[0],
        (session.session_id, Some(1), 1)
    );

    // Cross-org access reads as absence.
    assert!(matches!(
        facade
            .append_events(&outsider, session.session_id, vec![message()])
            .await,
        Err(LedgerError::SessionNotFound)
    ));

    // Missing scope is a typed error.
    assert!(matches!(
        facade
            .append_events(&unscoped, session.session_id, vec![message()])
            .await,
        Err(LedgerError::MissingScope { .. })
    ));
}

#[tokio::test]
async fn thread_lookup_is_org_scoped() {
    let facade = AgentLedgerFacade::new(FakeLedger::default(), FakeMappings::default());
    let owner = agent(Some(1), &["ledger:append", "ledger:query"]);
    let outsider = agent(Some(2), &["ledger:query"]);

    facade
        .open_session(
            &owner,
            OpenSession {
                runtime_conversation_id: "conv-1".to_string(),
                external_thread_kind: Some(ExternalThreadKind::SlackThread),
                external_thread_key: Some("C1:171.1".to_string()),
            },
        )
        .await
        .unwrap();

    assert!(
        facade
            .find_session_by_thread(&owner, ExternalThreadKind::SlackThread, "C1:171.1")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        facade
            .find_session_by_thread(&outsider, ExternalThreadKind::SlackThread, "C1:171.1")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn mismatched_thread_fields_are_rejected() {
    let facade = AgentLedgerFacade::new(FakeLedger::default(), FakeMappings::default());
    let agent = agent(Some(1), &["ledger:append"]);
    assert!(matches!(
        facade
            .open_session(
                &agent,
                OpenSession {
                    runtime_conversation_id: "conv-1".to_string(),
                    external_thread_kind: Some(ExternalThreadKind::SlackThread),
                    external_thread_key: None,
                },
            )
            .await,
        Err(LedgerError::InvalidRequest(_))
    ));
}
