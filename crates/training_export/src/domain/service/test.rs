use super::*;
use crate::domain::model::{Projection, SharingMode};
use crate::domain::ports::{ExportJobRepo, LedgerReader};
use agent_ledger::domain::model::{
    Actor, ActorKind, AgentEvent, AgentEventPayload, RequestHeader, UserMessageSource,
};
use agent_ledger::domain::ports::EventFilter;
use chrono::Utc;
use macro_uuid::Uuid;
use std::sync::{Arc, Mutex};

fn event_on(session_id: Uuid, seq: i64, payload: AgentEventPayload) -> AgentEvent {
    AgentEvent {
        session_id,
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

fn event(seq: i64, payload: AgentEventPayload) -> AgentEvent {
    event_on(Uuid::nil(), seq, payload)
}

fn request_header(composition_id: &str) -> AgentEventPayload {
    AgentEventPayload::RequestHeader(RequestHeader {
        rendered_system_prompt: "sys".into(),
        tool_schemas: serde_json::json!([]),
        provider: "anthropic".into(),
        model: "claude".into(),
        sampling: serde_json::json!({}),
        skill_versions: serde_json::json!({}),
        composition_id: composition_id.into(),
    })
}

fn user_message(content: &str) -> AgentEventPayload {
    AgentEventPayload::UserMessage {
        content: content.into(),
        source: UserMessageSource::Human,
    }
}

fn assistant_message(content: &str) -> AgentEventPayload {
    AgentEventPayload::AssistantMessage {
        content: content.into(),
        provider: "anthropic".into(),
        model: "claude".into(),
        usage: None,
    }
}

#[test]
fn model_history_drops_compacted_range() {
    let events = vec![
        event(0, user_message("hi")),
        event(1, assistant_message("old")),
        event(
            2,
            AgentEventPayload::Compaction {
                replaced_from_seq: 0,
                replaced_to_seq: 1,
                summary: "summary".into(),
            },
        ),
        event(3, user_message("next")),
    ];
    let projected = project_events(&events, Projection::ModelHistory, SharingMode::Full);
    let seqs: Vec<i64> = projected.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![2, 3]);
}

#[test]
fn model_history_compaction_is_per_session() {
    let session_a = macro_uuid::generate_uuid_v7();
    let session_b = macro_uuid::generate_uuid_v7();
    let events = vec![
        event_on(session_a, 0, user_message("a0")),
        event_on(
            session_a,
            1,
            AgentEventPayload::Compaction {
                replaced_from_seq: 0,
                replaced_to_seq: 0,
                summary: "summary".into(),
            },
        ),
        event_on(session_b, 0, user_message("b0")),
    ];
    let projected = project_events(&events, Projection::ModelHistory, SharingMode::Full);
    let kept: Vec<(Uuid, i64)> = projected.iter().map(|e| (e.session_id, e.seq)).collect();
    assert!(
        !kept.contains(&(session_a, 0)),
        "session A seq 0 is compacted"
    );
    assert!(kept.contains(&(session_a, 1)));
    assert!(
        kept.contains(&(session_b, 0)),
        "session B seq 0 must not inherit A's compaction"
    );
}

#[test]
fn human_transcript_keeps_only_user_and_assistant() {
    let events = vec![
        event(0, AgentEventPayload::TurnStart { turn: 0 }),
        event(1, user_message("hi")),
        event(
            2,
            AgentEventPayload::ToolCall {
                call_id: "c1".into(),
                name: "search".into(),
                arguments_raw: "{}".into(),
            },
        ),
        event(3, assistant_message("hello")),
    ];
    let projected = project_events(&events, Projection::HumanTranscript, SharingMode::Full);
    let types: Vec<&str> = projected.iter().map(|e| e.event_type.as_str()).collect();
    assert_eq!(types, vec!["user/message", "assistant/message"]);
}

#[test]
fn human_transcript_keeps_compacted_messages() {
    let events = vec![
        event(0, user_message("old")),
        event(
            1,
            AgentEventPayload::Compaction {
                replaced_from_seq: 0,
                replaced_to_seq: 0,
                summary: "summary".into(),
            },
        ),
        event(2, user_message("new")),
    ];
    let projected = project_events(&events, Projection::HumanTranscript, SharingMode::Full);
    let seqs: Vec<i64> = projected.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![0, 2]);
}

#[test]
fn training_export_is_full_trajectory() {
    let events = vec![
        event(0, AgentEventPayload::TurnStart { turn: 0 }),
        event(1, user_message("hi")),
        event(
            2,
            AgentEventPayload::ToolCall {
                call_id: "c1".into(),
                name: "search".into(),
                arguments_raw: "{}".into(),
            },
        ),
        event(
            3,
            AgentEventPayload::Compaction {
                replaced_from_seq: 1,
                replaced_to_seq: 2,
                summary: "summary".into(),
            },
        ),
    ];
    let projected = project_events(&events, Projection::TrainingExport, SharingMode::Full);
    let seqs: Vec<i64> = projected.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![0, 1, 2, 3]);
}

#[test]
fn feedback_only_consent_strips_everything_but_feedback() {
    let events = vec![
        event(0, user_message("secret")),
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
fn feedback_only_human_transcript_emits_nothing() {
    let events = vec![
        event(0, user_message("hi")),
        event(
            1,
            AgentEventPayload::FeedbackRecord {
                rating: Some(true),
                note: None,
                target_seq: Some(0),
            },
        ),
    ];
    let projected = project_events(
        &events,
        Projection::HumanTranscript,
        SharingMode::FeedbackOnly,
    );
    assert!(projected.is_empty());
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
        event(1, user_message("hi")),
    ];
    let projected = project_events(&events, Projection::TrainingExport, SharingMode::Full);
    assert_eq!(projected[0].parent_session_id, Some(parent));
    assert_eq!(projected[1].parent_session_id, Some(parent));
}

#[test]
fn session_seed_lineage_does_not_leak_across_sessions() {
    let parent = macro_uuid::generate_uuid_v7();
    let forked = macro_uuid::generate_uuid_v7();
    let other = macro_uuid::generate_uuid_v7();
    let events = vec![
        event_on(
            forked,
            0,
            AgentEventPayload::SessionSeed {
                parent_session_id: parent,
                seed_length: 1,
            },
        ),
        event_on(other, 0, user_message("unrelated")),
    ];
    let projected = project_events(&events, Projection::TrainingExport, SharingMode::Full);
    let other_row = projected
        .iter()
        .find(|e| e.session_id == other)
        .expect("other session");
    assert_eq!(other_row.parent_session_id, None);
}

#[test]
fn disabled_consent_exports_nothing() {
    let events = vec![event(0, AgentEventPayload::TurnStart { turn: 0 })];
    let projected = project_events(&events, Projection::TrainingExport, SharingMode::Disabled);
    assert!(projected.is_empty());
}

#[test]
fn composition_is_carried_onto_later_rows_and_pre_header_rows() {
    let events = vec![
        event(0, user_message("before header")),
        event(1, request_header("super-agent/v1")),
        event(2, user_message("after header")),
    ];
    let projected = project_events(&events, Projection::TrainingExport, SharingMode::Full);
    assert!(
        projected
            .iter()
            .all(|e| e.composition_id.as_deref() == Some("super-agent/v1"))
    );
}

#[test]
fn composition_pin_drops_sessions_that_never_logged_a_header() {
    let pinned = macro_uuid::generate_uuid_v7();
    let orphan = macro_uuid::generate_uuid_v7();
    let events = vec![
        event_on(pinned, 0, request_header("super-agent/v1")),
        event_on(pinned, 1, user_message("keep")),
        event_on(orphan, 0, user_message("no header")),
    ];
    let projected = filter_composition_pin(
        project_events(&events, Projection::TrainingExport, SharingMode::Full),
        Some("super-agent/v1"),
    );
    assert!(projected.iter().all(|e| e.session_id == pinned));
    assert_eq!(projected.len(), 2);
}

#[test]
fn composition_pin_does_not_inherit_across_sessions() {
    let with_header = macro_uuid::generate_uuid_v7();
    let without = macro_uuid::generate_uuid_v7();
    // Newest-first, matching the ledger query order, so a running pointer
    // would leak the header onto `without`.
    let events = vec![
        event_on(without, 0, user_message("orphan")),
        event_on(with_header, 1, user_message("child")),
        event_on(with_header, 0, request_header("super-agent/v1")),
    ];
    let projected = filter_composition_pin(
        project_events(&events, Projection::TrainingExport, SharingMode::Full),
        Some("super-agent/v1"),
    );
    assert!(projected.iter().all(|e| e.session_id == with_header));
    assert!(!projected.iter().any(|e| e.session_id == without));
}

#[test]
fn composition_pin_keeps_pre_header_rows_of_a_matching_session() {
    let events = vec![
        event(0, user_message("before")),
        event(1, request_header("super-agent/v1")),
    ];
    let projected = filter_composition_pin(
        project_events(&events, Projection::TrainingExport, SharingMode::Full),
        Some("super-agent/v1"),
    );
    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0].seq, 0);
}

#[test]
fn composition_pin_drops_a_different_composition() {
    let events = vec![
        event(0, request_header("other/v1")),
        event(1, user_message("nope")),
    ];
    let projected = filter_composition_pin(
        project_events(&events, Projection::TrainingExport, SharingMode::Full),
        Some("super-agent/v1"),
    );
    assert!(projected.is_empty());
}

#[test]
fn empty_composition_pin_is_a_no_op() {
    let events = vec![event(0, user_message("keep"))];
    let projected = filter_composition_pin(
        project_events(&events, Projection::TrainingExport, SharingMode::Full),
        Some(""),
    );
    assert_eq!(projected.len(), 1);
}

#[test]
fn feedback_only_still_carries_header_and_seed_onto_feedback_rows() {
    let parent = macro_uuid::generate_uuid_v7();
    let events = vec![
        event(
            0,
            AgentEventPayload::SessionSeed {
                parent_session_id: parent,
                seed_length: 0,
            },
        ),
        event(1, request_header("super-agent/v1")),
        event(2, user_message("secret")),
        event(
            3,
            AgentEventPayload::FeedbackRecord {
                rating: Some(false),
                note: None,
                target_seq: Some(2),
            },
        ),
    ];
    let projected = project_events(
        &events,
        Projection::TrainingExport,
        SharingMode::FeedbackOnly,
    );
    assert_eq!(projected.len(), 1);
    assert_eq!(
        projected[0].composition_id.as_deref(),
        Some("super-agent/v1")
    );
    assert_eq!(projected[0].parent_session_id, Some(parent));
}

#[test]
fn projection_is_stable_when_ledger_returns_newest_first() {
    let events = vec![
        event(3, user_message("last")),
        event(
            2,
            AgentEventPayload::Compaction {
                replaced_from_seq: 0,
                replaced_to_seq: 1,
                summary: "summary".into(),
            },
        ),
        event(1, assistant_message("old")),
        event(0, user_message("first")),
    ];
    let projected = project_events(&events, Projection::ModelHistory, SharingMode::Full);
    let seqs: Vec<i64> = projected.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![2, 3]);
}

#[derive(Default, Clone)]
struct FakeLedger {
    events: Arc<Mutex<Vec<AgentEvent>>>,
    fail: bool,
}

impl LedgerReader for FakeLedger {
    async fn query_events(&self, _filter: EventFilter) -> Result<Vec<AgentEvent>> {
        if self.fail {
            return Err(crate::domain::model::ExportError::InvalidRequest(
                "ledger unavailable".into(),
            ));
        }
        Ok(self.events.lock().unwrap().clone())
    }
}

#[derive(Default, Clone)]
struct FakeJobs {
    rows: Arc<Mutex<Vec<ExportJob>>>,
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
    ledger
        .events
        .lock()
        .unwrap()
        .push(event(0, request_header("super-agent/v1")));
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

#[tokio::test]
async fn run_pin_excludes_sessions_without_a_header() {
    let with_header = macro_uuid::generate_uuid_v7();
    let without = macro_uuid::generate_uuid_v7();
    let ledger = FakeLedger::default();
    ledger.events.lock().unwrap().extend([
        event_on(without, 0, user_message("orphan")),
        event_on(with_header, 0, request_header("super-agent/v1")),
        event_on(with_header, 1, user_message("keep")),
    ]);
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
    assert_eq!(job.row_count, Some(2));
    assert!(rows.iter().all(|e| e.session_id == with_header));
}

#[tokio::test]
async fn run_marks_job_failed_when_ledger_errors() {
    let jobs = FakeJobs::default();
    let svc = ExportServiceImpl::new(
        FakeLedger {
            fail: true,
            ..FakeLedger::default()
        },
        jobs.clone(),
    );
    let err = svc
        .run(
            Some(1),
            Projection::TrainingExport,
            SharingMode::Full,
            None,
            None,
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        crate::domain::model::ExportError::InvalidRequest(_)
    ));
    let stored = jobs.rows.lock().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].status, "failed");
    assert!(stored[0].error.is_some());
}
