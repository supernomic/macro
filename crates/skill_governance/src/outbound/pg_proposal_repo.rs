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

fn parse_proposal(
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
) -> Result<SkillProposal> {
    Ok(SkillProposal {
        id,
        org_id,
        skill_id,
        kind: ProposalKind::parse(&kind)
            .ok_or_else(|| GovernanceError::InvalidRequest(format!("unknown kind: {kind}")))?,
        slug,
        target_scope: SkillScope::parse(&target_scope).ok_or_else(|| {
            GovernanceError::InvalidRequest(format!("unknown scope: {target_scope}"))
        })?,
        owner_user_id,
        owner_team_id,
        proposed_name,
        proposed_description,
        proposed_body,
        diff_summary,
        evidence,
        proposer_agent_id,
        proposer_user_id,
        status: ProposalStatus::parse(&status)
            .ok_or_else(|| GovernanceError::InvalidRequest(format!("unknown status: {status}")))?,
        assignee_user_id,
        assignee_team_id,
        snapshot_id,
        eval_run_id,
        eval_passed,
        decided_by,
        decision_note,
        decided_at,
        created_at,
        updated_at,
    })
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
        row.map(|r| {
            parse_proposal(
                r.id,
                r.org_id,
                r.skill_id,
                r.kind,
                r.slug,
                r.target_scope,
                r.owner_user_id,
                r.owner_team_id,
                r.proposed_name,
                r.proposed_description,
                r.proposed_body,
                r.diff_summary,
                r.evidence,
                r.proposer_agent_id,
                r.proposer_user_id,
                r.status,
                r.assignee_user_id,
                r.assignee_team_id,
                r.snapshot_id,
                r.eval_run_id,
                r.eval_passed,
                r.decided_by,
                r.decision_note,
                r.decided_at,
                r.created_at,
                r.updated_at,
            )
        })
        .transpose()
    }

    #[tracing::instrument(skip(self), err)]
    async fn list(&self, filter: &ProposalFilter) -> Result<Vec<SkillProposal>> {
        let limit = if filter.limit <= 0 { 100 } else { filter.limit };
        let status = filter.status.map(|s| s.as_str().to_string());
        let rows = sqlx::query!(
            r#"
            SELECT id, org_id, skill_id, kind, slug, target_scope,
                   owner_user_id, owner_team_id, proposed_name, proposed_description,
                   proposed_body, diff_summary, evidence, proposer_agent_id,
                   proposer_user_id, status, assignee_user_id, assignee_team_id,
                   snapshot_id, eval_run_id, eval_passed, decided_by, decision_note,
                   decided_at, created_at, updated_at
            FROM agent_skill_proposals
            WHERE ($1::int IS NULL OR org_id IS NOT DISTINCT FROM $1)
              AND ($2::text IS NULL OR status = $2)
              AND ($3::text IS NULL OR assignee_user_id = $3)
              AND ($4::uuid IS NULL OR assignee_team_id = $4)
            ORDER BY created_at DESC
            LIMIT $5
            "#,
            filter.org_id,
            status,
            filter.assignee_user_id.as_deref(),
            filter.assignee_team_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                parse_proposal(
                    r.id,
                    r.org_id,
                    r.skill_id,
                    r.kind,
                    r.slug,
                    r.target_scope,
                    r.owner_user_id,
                    r.owner_team_id,
                    r.proposed_name,
                    r.proposed_description,
                    r.proposed_body,
                    r.diff_summary,
                    r.evidence,
                    r.proposer_agent_id,
                    r.proposer_user_id,
                    r.status,
                    r.assignee_user_id,
                    r.assignee_team_id,
                    r.snapshot_id,
                    r.eval_run_id,
                    r.eval_passed,
                    r.decided_by,
                    r.decision_note,
                    r.decided_at,
                    r.created_at,
                    r.updated_at,
                )
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
