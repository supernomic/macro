use super::*;
use crate::domain::model::{Projection, SharingMode};
use crate::domain::ports::{ExportJobRepo, LedgerReader};
use agent_ledger::domain::model::{
    Actor, ActorKind, AgentEvent, AgentEventPayload, RequestHeader, UserMessageSource,
};
use agent_ledger::domain::ports::EventFilter;
use chrono::Utc;
use macro_uuid::Uuid;
use std::sync::Mutex;

fn event(seq: i64, payload: AgentEventPayload) -> AgentEvent {
    AgentEvent {
        session_id: Uuid::nil(),
        seq,
        payload,
        actor: Actor {
            kind: ActorKind::System,
            id: "test".into(),
        },
        org_id: Some(1),
        occurred_at: Utc::now(),
        source_event_seqs: vec![],
        prev_hash: vec![0; 32],
        hash: vec![0; 32],
    }
}

#[test]
fn model_history_drops_compacted_range() {
    let events = vec![
        event(
            0,
            AgentEventPayload::UserMessage {
                content: "hi".into(),
                source: UserMessageSource::Human,
            },
        ),
        event(
            1,
            AgentEventPayload::AssistantMessage {
                content: "old".into(),
                provider: "anthropic".into(),
                model: "claude".into(),
                usage: None,
            },
        ),
        event(
            2,
            AgentEventPayload::Compaction {
                replaced_from_seq: 0,
                replaced_to_seq: 1,
                summary: "summary".into(),
            },
        ),
        event(
            3,
            AgentEventPayload::UserMessage {
                content: "next".into(),
                source: UserMessageSource::Human,
            },
        ),
    ];
    let projected = project_events(&events, Projection::ModelHistory, SharingMode::Full);
    let seqs: Vec<i64> = projected.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![2, 3]);
}

#[test]
fn human_transcript_keeps_only_messages() {
    let events = vec![
        event(0, AgentEventPayload::TurnStart { turn: 0 }),
        event(
            1,
            AgentEventPayload::UserMessage {
                content: "hi".into(),
                source: UserMessageSource::Human,
            },
        ),
        event(
            2,
            AgentEventPayload::ToolCall {
                call_id: "c1".into(),
                name: "search".into(),
                arguments_raw: "{}".into(),
            },
        ),
    ];
    let projected = project_events(&events, Projection::HumanTranscript, SharingMode::Full);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].event_type, "user/message");
}

#[test]
fn feedback_only_consent_strips_everything_but_feedback() {
    let events = vec![
        event(
            0,
            AgentEventPayload::UserMessage {
                content: "secret".into(),
                source: UserMessageSource::Human,
            },
        ),
        event(
            1,
            AgentEventPayload::FeedbackRecord {
                rating: Some(true),
                note: Some("good".into()),
                target_seq: Some(0),
            },
        ),
    ];
    let projected = project_events(
        &events,
        Projection::TrainingExport,
        SharingMode::FeedbackOnly,
    );
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].event_type, "feedback/record");
}

#[test]
fn session_seed_carries_fork_lineage() {
    let parent = Uuid::nil();
    let events = vec![
        event(
            0,
            AgentEventPayload::SessionSeed {
                parent_session_id: parent,
                seed_length: 4,
            },
        ),
        event(
            1,
            AgentEventPayload::UserMessage {
                content: "hi".into(),
                source: UserMessageSource::Human,
            },
        ),
    ];
    let projected = project_events(&events, Projection::TrainingExport, SharingMode::Full);
    assert_eq!(projected[1].parent_session_id, Some(parent));
}

#[test]
fn disabled_consent_exports_nothing() {
    let events = vec![event(0, AgentEventPayload::TurnStart { turn: 0 })];
    let projected = project_events(&events, Projection::TrainingExport, SharingMode::Disabled);
    assert!(projected.is_empty());
}

#[derive(Default)]
struct FakeLedger {
    events: Mutex<Vec<AgentEvent>>,
}

impl LedgerReader for FakeLedger {
    async fn query_events(&self, _filter: EventFilter) -> Result<Vec<AgentEvent>> {
        Ok(self.events.lock().unwrap().clone())
    }
}

#[derive(Default)]
struct FakeJobs {
    rows: Mutex<Vec<ExportJob>>,
}

impl ExportJobRepo for FakeJobs {
    async fn insert(&self, job: &ExportJob) -> Result<()> {
        self.rows.lock().unwrap().push(job.clone());
        Ok(())
    }

    async fn update(&self, job: &ExportJob) -> Result<()> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(existing) = rows.iter_mut().find(|j| j.id == job.id) {
            *existing = job.clone();
        }
        Ok(())
    }
}

#[tokio::test]
async fn run_records_a_completed_job() {
    let ledger = FakeLedger::default();
    ledger.events.lock().unwrap().push(event(
        0,
        AgentEventPayload::RequestHeader(RequestHeader {
            rendered_system_prompt: "sys".into(),
            tool_schemas: serde_json::json!([]),
            provider: "anthropic".into(),
            model: "claude".into(),
            sampling: serde_json::json!({}),
            skill_versions: serde_json::json!({}),
            composition_id: "super-agent/v1".into(),
        }),
    ));
    let svc = ExportServiceImpl::new(ledger, FakeJobs::default());
    let (job, rows) = svc
        .run(
            Some(1),
            Projection::TrainingExport,
            SharingMode::Full,
            Some("super-agent/v1".into()),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(job.status, "completed");
    assert_eq!(job.row_count, Some(1));
    assert_eq!(rows[0].composition_id.as_deref(), Some("super-agent/v1"));
}
