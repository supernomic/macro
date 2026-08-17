//! Postgres implementation of skill-proposal storage.

use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{
    GovernanceError, ProposalKind, ProposalStatus, Result, SkillEvalRun, SkillProposal, SkillScope,
    TraceRefinement,
};
use crate::domain::ports::{ProposalFilter, ProposalRepo};

/// Postgres-backed proposal repo.
#[derive(Debug, Clone)]
pub struct PgProposalRepo {
    pool: PgPool,
}

impl PgProposalRepo {
    /// Build over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct ProposalRow {
    id: Uuid,
    org_id: Option<i32>,
    skill_id: Option<Uuid>,
    kind: String,
    slug: String,
    target_scope: String,
    owner_user_id: Option<String>,
    owner_team_id: Option<Uuid>,
    proposed_name: String,
    proposed_description: String,
    proposed_body: String,
    diff_summary: String,
    evidence: serde_json::Value,
    proposer_agent_id: Option<String>,
    proposer_user_id: Option<String>,
    status: String,
    assignee_user_id: Option<String>,
    assignee_team_id: Option<Uuid>,
    snapshot_id: Option<Uuid>,
    eval_run_id: Option<String>,
    eval_passed: Option<bool>,
    decided_by: Option<String>,
    decision_note: Option<String>,
    decided_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

fn row_to_proposal(row: ProposalRow) -> Result<SkillProposal> {
    Ok(SkillProposal {
        id: row.id,
        org_id: row.org_id,
        skill_id: row.skill_id,
        kind: ProposalKind::parse(&row.kind).ok_or_else(|| {
            GovernanceError::InvalidRequest(format!("unknown kind: {}", row.kind))
        })?,
        slug: row.slug,
        target_scope: SkillScope::parse(&row.target_scope).ok_or_else(|| {
            GovernanceError::InvalidRequest(format!("unknown scope: {}", row.target_scope))
        })?,
        owner_user_id: row.owner_user_id,
        owner_team_id: row.owner_team_id,
        proposed_name: row.proposed_name,
        proposed_description: row.proposed_description,
        proposed_body: row.proposed_body,
        diff_summary: row.diff_summary,
        evidence: row.evidence,
        proposer_agent_id: row.proposer_agent_id,
        proposer_user_id: row.proposer_user_id,
        status: ProposalStatus::parse(&row.status).ok_or_else(|| {
            GovernanceError::InvalidRequest(format!("unknown status: {}", row.status))
        })?,
        assignee_user_id: row.assignee_user_id,
        assignee_team_id: row.assignee_team_id,
        snapshot_id: row.snapshot_id,
        eval_run_id: row.eval_run_id,
        eval_passed: row.eval_passed,
        decided_by: row.decided_by,
        decision_note: row.decision_note,
        decided_at: row.decided_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

macro_rules! proposal_from_record {
    ($r:expr) => {
        row_to_proposal(ProposalRow {
            id: $r.id,
            org_id: $r.org_id,
            skill_id: $r.skill_id,
            kind: $r.kind,
            slug: $r.slug,
            target_scope: $r.target_scope,
            owner_user_id: $r.owner_user_id,
            owner_team_id: $r.owner_team_id,
            proposed_name: $r.proposed_name,
            proposed_description: $r.proposed_description,
            proposed_body: $r.proposed_body,
            diff_summary: $r.diff_summary,
            evidence: $r.evidence,
            proposer_agent_id: $r.proposer_agent_id,
            proposer_user_id: $r.proposer_user_id,
            status: $r.status,
            assignee_user_id: $r.assignee_user_id,
            assignee_team_id: $r.assignee_team_id,
            snapshot_id: $r.snapshot_id,
            eval_run_id: $r.eval_run_id,
            eval_passed: $r.eval_passed,
            decided_by: $r.decided_by,
            decision_note: $r.decision_note,
            decided_at: $r.decided_at,
            created_at: $r.created_at,
            updated_at: $r.updated_at,
        })
    };
}

impl ProposalRepo for PgProposalRepo {
    #[tracing::instrument(skip(self, proposal), err)]
    async fn insert(&self, proposal: &SkillProposal) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO agent_skill_proposals (
                id, org_id, skill_id, kind, slug, target_scope,
                owner_user_id, owner_team_id, proposed_name, proposed_description,
                proposed_body, diff_summary, evidence, proposer_agent_id,
                proposer_user_id, status, assignee_user_id, assignee_team_id,
                snapshot_id, eval_run_id, eval_passed, decided_by, decision_note,
                decided_at, created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10,
                $11, $12, $13, $14,
                $15, $16, $17, $18,
                $19, $20, $21, $22, $23,
                $24, $25, $26
            )
            "#,
            proposal.id,
            proposal.org_id,
            proposal.skill_id,
            proposal.kind.as_str(),
            proposal.slug,
            proposal.target_scope.as_str(),
            proposal.owner_user_id.as_deref(),
            proposal.owner_team_id,
            proposal.proposed_name,
            proposal.proposed_description,
            proposal.proposed_body,
            proposal.diff_summary,
            proposal.evidence,
            proposal.proposer_agent_id.as_deref(),
            proposal.proposer_user_id.as_deref(),
            proposal.status.as_str(),
            proposal.assignee_user_id.as_deref(),
            proposal.assignee_team_id,
            proposal.snapshot_id,
            proposal.eval_run_id.as_deref(),
            proposal.eval_passed,
            proposal.decided_by.as_deref(),
            proposal.decision_note.as_deref(),
            proposal.decided_at,
            proposal.created_at,
            proposal.updated_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), err)]
    async fn get(&self, id: Uuid) -> Result<Option<SkillProposal>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, skill_id, kind, slug, target_scope,
                   owner_user_id, owner_team_id, proposed_name, proposed_description,
                   proposed_body, diff_summary, evidence, proposer_agent_id,
                   proposer_user_id, status, assignee_user_id, assignee_team_id,
                   snapshot_id, eval_run_id, eval_passed, decided_by, decision_note,
                   decided_at, created_at, updated_at
            FROM agent_skill_proposals
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| proposal_from_record!(r)).transpose()
    }

    #[tracing::instrument(skip(self), err)]
    async fn list(&self, filter: &ProposalFilter) -> Result<Vec<SkillProposal>> {
        // Dynamic filter combination over a fixed column set (unassigned_only
        // is optional, matching approvals team-queue HITL).
        const SELECT_COLUMNS: &str = "id, org_id, skill_id, kind, slug, target_scope, \
             owner_user_id, owner_team_id, proposed_name, proposed_description, \
             proposed_body, diff_summary, evidence, proposer_agent_id, \
             proposer_user_id, status, assignee_user_id, assignee_team_id, \
             snapshot_id, eval_run_id, eval_passed, decided_by, decision_note, \
             decided_at, created_at, updated_at";
        let mut builder = sqlx::QueryBuilder::new(format!(
            "SELECT {SELECT_COLUMNS} FROM agent_skill_proposals WHERE TRUE"
        ));
        if let Some(org_id) = filter.org_id {
            builder
                .push(" AND org_id IS NOT DISTINCT FROM ")
                .push_bind(org_id);
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
        if let Some(slug) = &filter.slug {
            builder.push(" AND slug = ").push_bind(slug.clone());
        }
        builder
            .push(" ORDER BY created_at DESC LIMIT ")
            .push_bind(if filter.limit > 0 { filter.limit } else { 100 });

        let rows = builder.build().fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|row| {
                use sqlx::Row;
                row_to_proposal(ProposalRow {
                    id: row.try_get("id")?,
                    org_id: row.try_get("org_id")?,
                    skill_id: row.try_get("skill_id")?,
                    kind: row.try_get("kind")?,
                    slug: row.try_get("slug")?,
                    target_scope: row.try_get("target_scope")?,
                    owner_user_id: row.try_get("owner_user_id")?,
                    owner_team_id: row.try_get("owner_team_id")?,
                    proposed_name: row.try_get("proposed_name")?,
                    proposed_description: row.try_get("proposed_description")?,
                    proposed_body: row.try_get("proposed_body")?,
                    diff_summary: row.try_get("diff_summary")?,
                    evidence: row.try_get("evidence")?,
                    proposer_agent_id: row.try_get("proposer_agent_id")?,
                    proposer_user_id: row.try_get("proposer_user_id")?,
                    status: row.try_get("status")?,
                    assignee_user_id: row.try_get("assignee_user_id")?,
                    assignee_team_id: row.try_get("assignee_team_id")?,
                    snapshot_id: row.try_get("snapshot_id")?,
                    eval_run_id: row.try_get("eval_run_id")?,
                    eval_passed: row.try_get("eval_passed")?,
                    decided_by: row.try_get("decided_by")?,
                    decision_note: row.try_get("decision_note")?,
                    decided_at: row.try_get("decided_at")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect()
    }

    #[tracing::instrument(skip(self, proposal), err)]
    async fn update(&self, proposal: &SkillProposal) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE agent_skill_proposals SET
                skill_id = $2,
                status = $3,
                snapshot_id = $4,
                eval_run_id = $5,
                eval_passed = $6,
                decided_by = $7,
                decision_note = $8,
                decided_at = $9,
                updated_at = $10
            WHERE id = $1
            "#,
            proposal.id,
            proposal.skill_id,
            proposal.status.as_str(),
            proposal.snapshot_id,
            proposal.eval_run_id.as_deref(),
            proposal.eval_passed,
            proposal.decided_by.as_deref(),
            proposal.decision_note.as_deref(),
            proposal.decided_at,
            proposal.updated_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self, run), err)]
    async fn insert_eval(&self, run: &SkillEvalRun) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO agent_skill_eval_runs (
                id, org_id, skill_id, proposal_id, composition_id, dataset,
                passed, score, report, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
            run.id,
            run.org_id,
            run.skill_id,
            run.proposal_id,
            run.composition_id,
            run.dataset,
            run.passed,
            run.score,
            run.report,
            run.created_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), err)]
    async fn latest_eval_for_proposal(&self, proposal_id: Uuid) -> Result<Option<SkillEvalRun>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, skill_id, proposal_id, composition_id, dataset,
                   passed, score, report, created_at
            FROM agent_skill_eval_runs
            WHERE proposal_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            proposal_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| SkillEvalRun {
            id: r.id,
            org_id: r.org_id,
            skill_id: r.skill_id,
            proposal_id: r.proposal_id,
            composition_id: r.composition_id,
            dataset: r.dataset,
            passed: r.passed,
            score: r.score,
            report: r.report,
            created_at: r.created_at,
        }))
    }

    #[tracing::instrument(skip(self, job), err)]
    async fn insert_refinement(&self, job: &TraceRefinement) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO agent_trace_refinements (
                id, org_id, proposal_id, session_id, window_start, window_end,
                evidence_excerpt, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            job.id,
            job.org_id,
            job.proposal_id,
            job.session_id,
            job.window_start,
            job.window_end,
            job.evidence_excerpt,
            job.created_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
