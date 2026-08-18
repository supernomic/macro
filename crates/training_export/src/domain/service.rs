//! Training-export domain service: three ledger projections.

#[cfg(test)]
mod test;

use std::collections::HashMap;

use agent_ledger::domain::model::{AgentEvent, AgentEventPayload};
use agent_ledger::domain::ports::EventFilter;
use chrono::Utc;
use macro_uuid::Uuid;

use super::model::{ExportJob, ProjectedEvent, Projection, Result, SharingMode};
use super::ports::{ConsentReader, ExportJobRepo, LedgerReader};

/// Domain service.
pub trait ExportService: Send + Sync + 'static {
    /// Run a projection over the requested window and record the job.
    fn run(
        &self,
        org_id: Option<i32>,
        projection: Projection,
        sharing_mode: SharingMode,
        composition_id: Option<String>,
        from_occurred_at: Option<chrono::DateTime<Utc>>,
        to_occurred_at: Option<chrono::DateTime<Utc>>,
    ) -> impl Future<Output = Result<(ExportJob, Vec<ProjectedEvent>)>> + Send;
}

/// Per-session facts collected from the ledger before emitting rows.
///
/// Ledger queries return `occurred_at DESC`, so projection must not walk a
/// running composition/parent pointer across the mixed stream.
struct SessionMeta {
    /// `(seq, composition_id)` from each `request/header`, sorted by seq.
    headers: Vec<(i64, String)>,
    /// `(seq, parent_session_id)` from `session/seed`, when present.
    seed: Option<(i64, Uuid)>,
    /// Inclusive seq ranges replaced by compaction.
    compacted: Vec<(i64, i64)>,
}

impl SessionMeta {
    fn composition_at(&self, seq: i64) -> Option<String> {
        if self.headers.is_empty() {
            return None;
        }
        // Latest header at or before this seq; events before the first header
        // inherit that first composition so a pin keeps the whole session.
        self.headers
            .iter()
            .rev()
            .find(|(header_seq, _)| *header_seq <= seq)
            .or(self.headers.first())
            .map(|(_, composition_id)| composition_id.clone())
    }

    fn parent_at(&self, seq: i64) -> Option<Uuid> {
        self.seed
            .filter(|(seed_seq, _)| seq >= *seed_seq)
            .map(|(_, parent)| parent)
    }

    fn is_compacted(&self, seq: i64) -> bool {
        self.compacted
            .iter()
            .any(|(from, to)| seq >= *from && seq <= *to)
    }
}

fn collect_session_meta(events: &[AgentEvent]) -> HashMap<Uuid, SessionMeta> {
    let mut sessions: HashMap<Uuid, SessionMeta> = HashMap::new();
    for event in events {
        let meta = sessions.entry(event.session_id).or_insert(SessionMeta {
            headers: Vec::new(),
            seed: None,
            compacted: Vec::new(),
        });
        match &event.payload {
            AgentEventPayload::RequestHeader(header) => {
                meta.headers
                    .push((event.seq, header.composition_id.clone()));
            }
            AgentEventPayload::SessionSeed {
                parent_session_id, ..
            } => {
                meta.seed = Some((event.seq, *parent_session_id));
            }
            AgentEventPayload::Compaction {
                replaced_from_seq,
                replaced_to_seq,
                ..
            } => {
                meta.compacted.push((*replaced_from_seq, *replaced_to_seq));
            }
            _ => {}
        }
    }
    for meta in sessions.values_mut() {
        meta.headers.sort_by_key(|(seq, _)| *seq);
    }
    sessions
}

/// Keep rows whose carried `composition_id` matches `pin`.
///
/// Sessions that never logged `request/header` have `composition_id = None`
/// and are excluded — a pin cannot match an unknown composition. Cross-session
/// leakage is avoided because composition is resolved per session.
pub fn filter_composition_pin(rows: Vec<ProjectedEvent>, pin: Option<&str>) -> Vec<ProjectedEvent> {
    let Some(pin) = pin.filter(|s| !s.is_empty()) else {
        return rows;
    };
    rows.into_iter()
        .filter(|row| row.composition_id.as_deref() == Some(pin))
        .collect()
}

/// Effective mode is `min(job_mode, session_consent)`.
///
/// Missing consent skips the session. Either side `Disabled` skips.
/// The job cannot raise sharing above the session's consent.
pub fn effective_sharing(
    job_mode: SharingMode,
    session_consent: Option<SharingMode>,
) -> Option<SharingMode> {
    let session = session_consent?;
    let mode = job_mode.min(session);
    (mode != SharingMode::Disabled).then_some(mode)
}

/// Project events using a single already-resolved sharing mode.
pub fn project_events(
    events: &[AgentEvent],
    projection: Projection,
    sharing_mode: SharingMode,
) -> Vec<ProjectedEvent> {
    project_with_session_mode(events, projection, |_| Some(sharing_mode))
}

/// Project events applying per-session consent against the job sharing mode.
///
/// Sessions absent from `consents` are skipped.
pub fn project_consented_events(
    events: &[AgentEvent],
    projection: Projection,
    job_mode: SharingMode,
    consents: &HashMap<Uuid, SharingMode>,
) -> Vec<ProjectedEvent> {
    project_with_session_mode(events, projection, |session_id| {
        effective_sharing(job_mode, consents.get(&session_id).copied())
    })
}

fn project_with_session_mode(
    events: &[AgentEvent],
    projection: Projection,
    session_mode: impl Fn(Uuid) -> Option<SharingMode>,
) -> Vec<ProjectedEvent> {
    let sessions = collect_session_meta(events);
    let mut ordered: Vec<&AgentEvent> = events.iter().collect();
    ordered.sort_by_key(|event| (event.session_id, event.seq));

    let mut out = Vec::new();
    for event in ordered {
        let Some(sharing_mode) = session_mode(event.session_id) else {
            continue;
        };
        if sharing_mode == SharingMode::Disabled {
            continue;
        }
        if sharing_mode == SharingMode::FeedbackOnly
            && !matches!(event.payload, AgentEventPayload::FeedbackRecord { .. })
        {
            continue;
        }
        let meta = sessions.get(&event.session_id);
        let include = match projection {
            Projection::HumanTranscript => matches!(
                event.payload,
                AgentEventPayload::UserMessage { .. } | AgentEventPayload::AssistantMessage { .. }
            ),
            Projection::ModelHistory => meta.is_none_or(|m| !m.is_compacted(event.seq)),
            Projection::TrainingExport => true,
        };
        if !include {
            continue;
        }
        out.push(ProjectedEvent {
            session_id: event.session_id,
            seq: event.seq,
            event_type: event.payload.event_type().to_string(),
            data: serde_json::to_value(&event.payload).unwrap_or(serde_json::json!({})),
            composition_id: meta.and_then(|m| m.composition_at(event.seq)),
            parent_session_id: meta.and_then(|m| m.parent_at(event.seq)),
        });
    }
    out
}

fn unique_session_ids(events: &[AgentEvent]) -> Vec<Uuid> {
    let mut ids: Vec<Uuid> = events.iter().map(|event| event.session_id).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Concrete service.
#[derive(Debug, Clone)]
pub struct ExportServiceImpl<L, J, C> {
    ledger: L,
    jobs: J,
    consent: C,
}

impl<L: LedgerReader, J: ExportJobRepo, C: ConsentReader> ExportServiceImpl<L, J, C> {
    /// Build over ledger, job storage, and per-session consent.
    ///
    /// `consent` is required. Sessions with no consent row are skipped and
    /// never treated as Full.
    pub fn new(ledger: L, jobs: J, consent: C) -> Self {
        Self {
            ledger,
            jobs,
            consent,
        }
    }
}

impl<L: LedgerReader, J: ExportJobRepo, C: ConsentReader> ExportService
    for ExportServiceImpl<L, J, C>
{
    #[tracing::instrument(skip(self), err)]
    async fn run(
        &self,
        org_id: Option<i32>,
        projection: Projection,
        sharing_mode: SharingMode,
        composition_id: Option<String>,
        from_occurred_at: Option<chrono::DateTime<Utc>>,
        to_occurred_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<(ExportJob, Vec<ProjectedEvent>)> {
        let mut job = ExportJob {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            projection,
            sharing_mode,
            composition_id: composition_id.clone(),
            from_occurred_at,
            to_occurred_at,
            status: "running".to_string(),
            row_count: None,
            artifact_uri: None,
            error: None,
            created_at: Utc::now(),
            completed_at: None,
        };
        self.jobs.insert(&job).await?;
        let result: Result<Vec<ProjectedEvent>> = async {
            let events = self
                .ledger
                .query_events(EventFilter {
                    org_id,
                    occurred_after: from_occurred_at,
                    occurred_before: to_occurred_at,
                    limit: 10_000,
                    ..Default::default()
                })
                .await?;
            let consents = self
                .consent
                .sharing_modes(&unique_session_ids(&events))
                .await?;
            let projected = filter_composition_pin(
                project_consented_events(&events, projection, sharing_mode, &consents),
                composition_id.as_deref(),
            );
            Ok(projected)
        }
        .await;
        match result {
            Ok(projected) => {
                job.status = "completed".to_string();
                job.row_count = Some(projected.len() as i64);
                job.completed_at = Some(Utc::now());
                self.jobs.update(&job).await?;
                Ok((job, projected))
            }
            Err(e) => {
                job.status = "failed".to_string();
                job.error = Some(e.to_string());
                job.completed_at = Some(Utc::now());
                if let Err(update_err) = self.jobs.update(&job).await {
                    tracing::error!(error=?update_err, "failed to persist export job failure");
                }
                Err(e)
            }
        }
    }
}
