//! Training-export domain service: three ledger projections.

#[cfg(test)]
mod test;

use agent_ledger::domain::model::{AgentEvent, AgentEventPayload};
use agent_ledger::domain::ports::EventFilter;
use chrono::Utc;

use super::model::{ExportJob, ProjectedEvent, Projection, Result, SharingMode};
use super::ports::{ExportJobRepo, LedgerReader};

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

/// Project a session's events according to `projection` and `sharing_mode`.
pub fn project_events(
    events: &[AgentEvent],
    projection: Projection,
    sharing_mode: SharingMode,
) -> Vec<ProjectedEvent> {
    if sharing_mode == SharingMode::Disabled {
        return Vec::new();
    }
    let mut compacted: Vec<(i64, i64)> = Vec::new();
    for event in events {
        if let AgentEventPayload::Compaction {
            replaced_from_seq,
            replaced_to_seq,
            ..
        } = &event.payload
        {
            compacted.push((*replaced_from_seq, *replaced_to_seq));
        }
    }
    let mut composition_id: Option<String> = None;
    let mut parent_session_id: Option<macro_uuid::Uuid> = None;
    let mut out = Vec::new();
    for event in events {
        if let AgentEventPayload::RequestHeader(header) = &event.payload {
            composition_id = Some(header.composition_id.clone());
        }
        if let AgentEventPayload::SessionSeed {
            parent_session_id: parent,
            ..
        } = &event.payload
        {
            parent_session_id = Some(*parent);
        }
        if sharing_mode == SharingMode::FeedbackOnly
            && !matches!(event.payload, AgentEventPayload::FeedbackRecord { .. })
        {
            continue;
        }
        let include = match projection {
            Projection::HumanTranscript => matches!(
                event.payload,
                AgentEventPayload::UserMessage { .. } | AgentEventPayload::AssistantMessage { .. }
            ),
            Projection::ModelHistory => !compacted
                .iter()
                .any(|(from, to)| event.seq >= *from && event.seq <= *to),
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
            composition_id: composition_id.clone(),
            parent_session_id,
        });
    }
    out
}

/// Concrete service.
#[derive(Debug, Clone)]
pub struct ExportServiceImpl<L, J> {
    ledger: L,
    jobs: J,
}

impl<L: LedgerReader, J: ExportJobRepo> ExportServiceImpl<L, J> {
    /// Build over ledger + job storage.
    pub fn new(ledger: L, jobs: J) -> Self {
        Self { ledger, jobs }
    }
}

impl<L: LedgerReader, J: ExportJobRepo> ExportService for ExportServiceImpl<L, J> {
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
        let mut projected = project_events(&events, projection, sharing_mode);
        if let Some(pin) = &composition_id {
            projected.retain(|e| e.composition_id.as_ref() == Some(pin));
        }
        job.status = "completed".to_string();
        job.row_count = Some(projected.len() as i64);
        job.completed_at = Some(Utc::now());
        self.jobs.update(&job).await?;
        Ok((job, projected))
    }
}
