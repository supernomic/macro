//! HTTP surface for skills governance.
//!
//! Agent principals (`mat_...`): catalog + propose.
//! Users: inbox list, get one proposal, decide, rollback.
//! Internal: record evals, ingest trace-refinement jobs.

use std::sync::Arc;

use agent_identity::domain::model::IdentityError;
use agent_identity::domain::ports::AgentIdentityService;
use agent_identity::inbound::AgentBearer;
use axum::extract::{FromRef, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use macro_authorization::{
    InternalOnly, MacroAuthorizationExtractor, MacroAuthorizationService, MacroAuthorizationState,
    UserOrInternal,
};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::facade::AgentSkillFacade;
use crate::domain::model::{
    GovernanceError, NewProposal, ProposalKind, SkillCatalogEntry, SkillEvalRun, SkillProposal,
    SkillRecord, SkillScope, TraceRefinement,
};
use crate::domain::service::{Caller, SkillGovernanceService, UserProposals};

/// Router state for the skills-governance surface.
pub struct SkillGovernanceRouterState<A, I, Auth> {
    /// Agent-facing facade.
    pub facade: Arc<AgentSkillFacade<A>>,
    /// User-facing service.
    pub service: Arc<A>,
    /// Identity service used to verify agent bearer tokens.
    pub identity: Arc<I>,
    /// Authorization state for user/internal extraction.
    pub authorization_state: MacroAuthorizationState<Auth>,
}

impl<A, I, Auth> Clone for SkillGovernanceRouterState<A, I, Auth> {
    fn clone(&self) -> Self {
        Self {
            facade: self.facade.clone(),
            service: self.service.clone(),
            identity: self.identity.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<A, I, Auth> FromRef<SkillGovernanceRouterState<A, I, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &SkillGovernanceRouterState<A, I, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Error body.
#[derive(Debug, Serialize, ToSchema)]
pub struct GovernanceErrorBody {
    /// Error description.
    pub error: String,
}

fn error_response(e: GovernanceError) -> Response {
    let status = match &e {
        GovernanceError::NotFound => StatusCode::NOT_FOUND,
        GovernanceError::InvalidStatus(_) => StatusCode::CONFLICT,
        GovernanceError::EvalGate(_) => StatusCode::CONFLICT,
        GovernanceError::Forbidden(_) | GovernanceError::MissingScope { .. } => {
            StatusCode::FORBIDDEN
        }
        GovernanceError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
        GovernanceError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    if status == StatusCode::INTERNAL_SERVER_ERROR {
        tracing::error!(error = ?e, "skill governance endpoint failed");
        return (
            status,
            Json(GovernanceErrorBody {
                error: "internal error".to_string(),
            }),
        )
            .into_response();
    }
    (
        status,
        Json(GovernanceErrorBody {
            error: e.to_string(),
        }),
    )
        .into_response()
}

fn identity_error_response(e: IdentityError) -> Response {
    let status = match &e {
        IdentityError::MalformedToken => StatusCode::BAD_REQUEST,
        IdentityError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::UNAUTHORIZED,
    };
    (
        status,
        Json(GovernanceErrorBody {
            error: "agent authentication failed".to_string(),
        }),
    )
        .into_response()
}

/// Request body for opening a skill proposal.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ProposeSkillRequest {
    /// `create` | `patch` | `archive`.
    pub kind: ProposalKind,
    /// Existing skill, required for patch/archive.
    pub skill_id: Option<Uuid>,
    /// Target slug.
    pub slug: String,
    /// Target scope.
    pub target_scope: SkillScope,
    /// Owner user when targeting user scope.
    pub owner_user_id: Option<String>,
    /// Owner team when targeting team scope.
    pub owner_team_id: Option<Uuid>,
    /// Proposed name.
    pub proposed_name: String,
    /// Proposed catalog description.
    pub proposed_description: String,
    /// Proposed body.
    pub proposed_body: String,
    /// Human-readable diff summary.
    pub diff_summary: String,
    /// Trace excerpts / eval pointers.
    #[serde(default = "empty_array")]
    pub evidence: serde_json::Value,
    /// Direct assignee.
    pub assignee_user_id: Option<String>,
    /// Team queue.
    pub assignee_team_id: Option<Uuid>,
}

fn empty_array() -> serde_json::Value {
    serde_json::json!([])
}

impl From<ProposeSkillRequest> for NewProposal {
    fn from(body: ProposeSkillRequest) -> Self {
        NewProposal {
            kind: body.kind,
            skill_id: body.skill_id,
            slug: body.slug,
            target_scope: body.target_scope,
            owner_user_id: body.owner_user_id,
            owner_team_id: body.owner_team_id,
            proposed_name: body.proposed_name,
            proposed_description: body.proposed_description,
            proposed_body: body.proposed_body,
            diff_summary: body.diff_summary,
            evidence: body.evidence,
            assignee_user_id: body.assignee_user_id,
            assignee_team_id: body.assignee_team_id,
        }
    }
}

/// Request body for deciding a pending proposal.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DecideProposalRequest {
    /// `true` approves, `false` rejects.
    pub approved: bool,
    /// Optional note.
    pub note: Option<String>,
}

/// Request body for recording an eval run.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RecordEvalRequest {
    /// Skill under test, when it already exists.
    pub skill_id: Option<Uuid>,
    /// Proposal this run gates.
    pub proposal_id: Option<Uuid>,
    /// Pinned composition id.
    pub composition_id: String,
    /// Dataset / task set name.
    pub dataset: String,
    /// Whether the run passed.
    pub passed: bool,
    /// Optional numeric score.
    pub score: Option<f64>,
    /// Full report blob.
    #[serde(default)]
    pub report: serde_json::Value,
}

/// Request body for a trace-refinement ingest.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RefineRequest {
    /// The proposal to open from the evidence.
    pub proposal: ProposeSkillRequest,
    /// Window start.
    pub window_start: DateTime<Utc>,
    /// Window end.
    pub window_end: DateTime<Utc>,
    /// Session the evidence was drawn from.
    pub session_id: Option<Uuid>,
    /// Evidence excerpt.
    #[serde(default = "empty_array")]
    pub evidence_excerpt: serde_json::Value,
}

/// Personal inbox view.
#[derive(Debug, Serialize, ToSchema)]
pub struct UserProposalsResponse {
    /// Pending proposals assigned directly to the user.
    pub assigned: Vec<SkillProposal>,
    /// Pending proposals on the user's teams' queues.
    pub team_queue: Vec<SkillProposal>,
}

impl From<UserProposals> for UserProposalsResponse {
    fn from(v: UserProposals) -> Self {
        Self {
            assigned: v.assigned,
            team_queue: v.team_queue,
        }
    }
}

/// Build the skills-governance router.
pub fn skill_governance_router<A, I, Auth, S>(
    state: SkillGovernanceRouterState<A, I, Auth>,
) -> Router<S>
where
    A: SkillGovernanceService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
    S: Send + Sync + Clone + 'static,
{
    Router::new()
        .route(
            "/agent-skills",
            get(agent_catalog_handler::<A, I, Auth>).post(agent_propose_handler::<A, I, Auth>),
        )
        .route(
            "/agent-skills/{id}",
            get(agent_get_skill_handler::<A, I, Auth>),
        )
        .route(
            "/skill-proposals/mine",
            get(list_my_proposals_handler::<A, I, Auth>),
        )
        .route(
            "/skill-proposals/{id}",
            get(get_proposal_handler::<A, I, Auth>),
        )
        .route(
            "/skill-proposals/{id}/decide",
            post(decide_proposal_handler::<A, I, Auth>),
        )
        .route(
            "/skill-proposals/{id}/rollback",
            post(rollback_proposal_handler::<A, I, Auth>),
        )
        .route("/skill-evals", post(record_eval_handler::<A, I, Auth>))
        .route("/skill-refinements", post(refine_handler::<A, I, Auth>))
        .with_state(state)
}

/// List skills this agent may inject.
#[utoipa::path(
    get,
    path = "/agent-skills",
    responses((status = 200, description = "Skill catalog", body = Vec<SkillCatalogEntry>)),
    tag = "skills"
)]
#[tracing::instrument(skip_all)]
pub async fn agent_catalog_handler<A, I, Auth>(
    State(state): State<SkillGovernanceRouterState<A, I, Auth>>,
    bearer: AgentBearer,
) -> Response
where
    A: SkillGovernanceService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state.facade.catalog(&agent).await {
        Ok(catalog) => Json(catalog).into_response(),
        Err(e) => error_response(e),
    }
}

/// Fetch one skill. Agent callers.
#[utoipa::path(
    get,
    path = "/agent-skills/{id}",
    params(("id" = Uuid, Path, description = "Skill id")),
    responses((status = 200, description = "The skill", body = SkillRecord)),
    tag = "skills"
)]
#[tracing::instrument(skip_all)]
pub async fn agent_get_skill_handler<A, I, Auth>(
    State(state): State<SkillGovernanceRouterState<A, I, Auth>>,
    bearer: AgentBearer,
    Path(id): Path<Uuid>,
) -> Response
where
    A: SkillGovernanceService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state.facade.get_skill(&agent, id).await {
        Ok(skill) => Json(skill).into_response(),
        Err(e) => error_response(e),
    }
}

/// Open a staged skill proposal. Agent callers.
#[utoipa::path(
    post,
    path = "/agent-skills",
    request_body = ProposeSkillRequest,
    responses((status = 200, description = "The proposal", body = SkillProposal)),
    tag = "skills"
)]
#[tracing::instrument(skip_all)]
pub async fn agent_propose_handler<A, I, Auth>(
    State(state): State<SkillGovernanceRouterState<A, I, Auth>>,
    bearer: AgentBearer,
    Json(body): Json<ProposeSkillRequest>,
) -> Response
where
    A: SkillGovernanceService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state.facade.propose(&agent, body.into()).await {
        Ok(proposal) => Json(proposal).into_response(),
        Err(e) => error_response(e),
    }
}

/// Fetch one proposal visible to the caller.
#[utoipa::path(
    get,
    path = "/skill-proposals/{id}",
    params(("id" = Uuid, Path, description = "Proposal id")),
    responses(
        (status = 200, description = "The skill proposal", body = SkillProposal),
        (status = 404, description = "Not found", body = GovernanceErrorBody),
    ),
    tag = "skills"
)]
#[tracing::instrument(skip_all)]
pub async fn get_proposal_handler<A, I, Auth>(
    State(state): State<SkillGovernanceRouterState<A, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(id): Path<Uuid>,
) -> Response
where
    A: SkillGovernanceService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state.service.get_proposal(&caller, id).await {
        Ok(proposal) => Json(proposal).into_response(),
        Err(e) => error_response(e),
    }
}

/// Personal inbox of skill proposals.
#[utoipa::path(
    get,
    path = "/skill-proposals/mine",
    responses((status = 200, description = "Inbox view", body = UserProposalsResponse)),
    tag = "skills"
)]
#[tracing::instrument(skip_all)]
pub async fn list_my_proposals_handler<A, I, Auth>(
    State(state): State<SkillGovernanceRouterState<A, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
) -> Response
where
    A: SkillGovernanceService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let user_id = user.authorization.user.macro_user_id.as_ref().to_string();
    match state.service.list_for_user(&user_id).await {
        Ok(view) => Json(UserProposalsResponse::from(view)).into_response(),
        Err(e) => error_response(e),
    }
}

/// Approve or reject a pending proposal.
#[utoipa::path(
    post,
    path = "/skill-proposals/{id}/decide",
    request_body = DecideProposalRequest,
    responses((status = 200, description = "Updated proposal", body = SkillProposal)),
    tag = "skills"
)]
#[tracing::instrument(skip_all)]
pub async fn decide_proposal_handler<A, I, Auth>(
    State(state): State<SkillGovernanceRouterState<A, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(id): Path<Uuid>,
    Json(body): Json<DecideProposalRequest>,
) -> Response
where
    A: SkillGovernanceService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state
        .service
        .decide(&caller, id, body.approved, body.note)
        .await
    {
        Ok(proposal) => Json(proposal).into_response(),
        Err(e) => error_response(e),
    }
}

/// Roll an approved proposal back to its snapshot.
#[utoipa::path(
    post,
    path = "/skill-proposals/{id}/rollback",
    responses((status = 200, description = "Restored skill", body = SkillRecord)),
    tag = "skills"
)]
#[tracing::instrument(skip_all)]
pub async fn rollback_proposal_handler<A, I, Auth>(
    State(state): State<SkillGovernanceRouterState<A, I, Auth>>,
    user: MacroAuthorizationExtractor<Auth, UserOrInternal>,
    Path(id): Path<Uuid>,
) -> Response
where
    A: SkillGovernanceService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let caller = Caller::User(user.authorization.user.macro_user_id.as_ref().to_string());
    match state.service.rollback(&caller, id, None).await {
        Ok(skill) => Json(skill).into_response(),
        Err(e) => error_response(e),
    }
}

/// Record an eval run that may gate promotion. Internal callers.
#[utoipa::path(
    post,
    path = "/skill-evals",
    request_body = RecordEvalRequest,
    responses((status = 200, description = "Recorded eval", body = SkillEvalRun)),
    tag = "skills"
)]
#[tracing::instrument(skip_all)]
pub async fn record_eval_handler<A, I, Auth>(
    State(state): State<SkillGovernanceRouterState<A, I, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Json(body): Json<RecordEvalRequest>,
) -> Response
where
    A: SkillGovernanceService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let run = SkillEvalRun {
        id: macro_uuid::generate_uuid_v7(),
        org_id: None,
        skill_id: body.skill_id,
        proposal_id: body.proposal_id,
        composition_id: body.composition_id,
        dataset: body.dataset,
        passed: body.passed,
        score: body.score,
        report: body.report,
        created_at: Utc::now(),
    };
    match state.service.record_eval(run).await {
        Ok(run) => Json(run).into_response(),
        Err(e) => error_response(e),
    }
}

/// Ingest a trace-refinement job. Internal callers.
#[utoipa::path(
    post,
    path = "/skill-refinements",
    request_body = RefineRequest,
    responses((status = 200, description = "Refinement job + proposal", body = TraceRefinement)),
    tag = "skills"
)]
#[tracing::instrument(skip_all)]
pub async fn refine_handler<A, I, Auth>(
    State(state): State<SkillGovernanceRouterState<A, I, Auth>>,
    _internal: MacroAuthorizationExtractor<Auth, InternalOnly>,
    Json(body): Json<RefineRequest>,
) -> Response
where
    A: SkillGovernanceService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    match state
        .service
        .refine(
            None,
            body.proposal.into(),
            body.window_start,
            body.window_end,
            body.session_id,
            body.evidence_excerpt,
        )
        .await
    {
        Ok((job, _proposal)) => Json(job).into_response(),
        Err(e) => error_response(e),
    }
}
