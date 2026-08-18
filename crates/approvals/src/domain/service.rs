//! Approval-gate domain service: policy evaluation (floor + org, strictest
//! wins), gate semantics with approval consumption on retry, and the
//! decide/reassign/cancel lifecycle.

#[cfg(test)]
mod test;

use chrono::Utc;
use macro_uuid::Uuid;

use super::model::{
    ApprovalError, ApprovalRequest, ApprovalStatus, ApprovalTransition, GateOutcome, GateRequest,
    PolicyDecision, PolicyFloor, Result, ToolPolicy, arguments_digest,
};
use super::ports::{
    ApprovalCallbackClient, ApprovalFilter, ApprovalNotifier, ApprovalRepo, PolicyRepo,
    TeamMembershipPort,
};

const DEFAULT_LIST_LIMIT: i64 = 100;

/// A caller identity for user-facing operations. `internal` bypasses
/// assignment checks (service-to-service and admin paths).
#[derive(Debug, Clone)]
pub enum Caller {
    /// A Macro user.
    User(String),
    /// A trusted internal caller.
    Internal,
}

impl Caller {
    fn actor_id(&self) -> String {
        match self {
            Caller::User(id) => id.clone(),
            Caller::Internal => "internal".to_string(),
        }
    }
}

/// Personal inbox view: requests assigned to the user plus requests on
/// their teams' queues awaiting a decision.
#[derive(Debug, Clone)]
pub struct UserApprovals {
    /// Pending requests assigned directly to the user.
    pub assigned: Vec<ApprovalRequest>,
    /// Pending requests on the user's teams' queues (no direct assignee).
    pub team_queue: Vec<ApprovalRequest>,
}

/// Domain service exposed to inbound adapters.
pub trait ApprovalService: Send + Sync + 'static {
    /// Evaluate the policy decision for an agent + tool without side
    /// effects.
    fn evaluate(
        &self,
        org_id: Option<i32>,
        agent_slug: &str,
        tool_name: &str,
    ) -> impl Future<Output = Result<PolicyDecision>> + Send;

    /// Gate one proposed tool call: `allow` passes through (consuming a
    /// matching prior approval when one exists), `deny` refuses, and
    /// `require_approval` opens (or returns the already-open) pending
    /// request routed to its approver.
    fn gate(
        &self,
        org_id: Option<i32>,
        request: GateRequest,
        actor_id: &str,
    ) -> impl Future<Output = Result<GateOutcome>> + Send;

    /// Fetch one request visible to the caller.
    fn get(
        &self,
        caller: &Caller,
        id: Uuid,
    ) -> impl Future<Output = Result<ApprovalRequest>> + Send;

    /// The caller's personal inbox view.
    fn list_for_user(&self, user_id: &str) -> impl Future<Output = Result<UserApprovals>> + Send;

    /// Everything on one team's queue (member-only).
    fn list_for_team(
        &self,
        caller: &Caller,
        team_id: Uuid,
    ) -> impl Future<Output = Result<Vec<ApprovalRequest>>> + Send;

    /// Approve or deny a pending request; triggers the runtime callback.
    fn decide(
        &self,
        caller: &Caller,
        id: Uuid,
        approved: bool,
        note: Option<String>,
    ) -> impl Future<Output = Result<ApprovalRequest>> + Send;

    /// Reassign a pending request ("this isn't mine → team X"), with a
    /// required reason recorded in the trail.
    fn reassign(
        &self,
        caller: &Caller,
        id: Uuid,
        to_user: Option<String>,
        to_team: Option<Uuid>,
        reason: String,
    ) -> impl Future<Output = Result<ApprovalRequest>> + Send;

    /// Cancel a pending request without a decision.
    fn cancel(
        &self,
        caller: &Caller,
        id: Uuid,
    ) -> impl Future<Output = Result<ApprovalRequest>> + Send;

    /// A request's audited transition history.
    fn transitions(
        &self,
        caller: &Caller,
        id: Uuid,
    ) -> impl Future<Output = Result<Vec<ApprovalTransition>>> + Send;
}

/// Concrete approval service over the storage and delivery ports.
#[derive(Debug, Clone)]
pub struct ApprovalServiceImpl<R, P, T, CB, N> {
    repo: R,
    policies: P,
    teams: T,
    callback: CB,
    notifier: N,
    floor: PolicyFloor,
}

impl<R, P, T, CB, N> ApprovalServiceImpl<R, P, T, CB, N>
where
    R: ApprovalRepo,
    P: PolicyRepo,
    T: TeamMembershipPort,
    CB: ApprovalCallbackClient,
    N: ApprovalNotifier,
{
    /// Build the service over its ports and the built-in policy floor.
    pub fn new(
        repo: R,
        policies: P,
        teams: T,
        callback: CB,
        notifier: N,
        floor: PolicyFloor,
    ) -> Self {
        Self {
            repo,
            policies,
            teams,
            callback,
            notifier,
            floor,
        }
    }

    /// The winning org policy for an agent + tool: most specific match;
    /// ties resolve to the strictest decision.
    fn winning_policy<'a>(
        policies: &'a [ToolPolicy],
        agent_slug: &str,
        tool_name: &str,
    ) -> Option<&'a ToolPolicy> {
        policies
            .iter()
            .filter(|p| p.matches(agent_slug, tool_name))
            .max_by_key(|p| (p.specificity(), p.decision))
    }

    /// Whether the caller may decide/act on this request (assignee, member
    /// of the assigned team, or internal).
    async fn may_act(&self, caller: &Caller, request: &ApprovalRequest) -> Result<bool> {
        let user_id = match caller {
            Caller::Internal => return Ok(true),
            Caller::User(id) => id,
        };
        if request.assignee_user_id.as_deref() == Some(user_id.as_str()) {
            return Ok(true);
        }
        if let Some(team_id) = request.assignee_team_id {
            return self.teams.is_member(user_id, team_id).await;
        }
        Ok(false)
    }

    async fn record_transition(
        &self,
        before: &ApprovalRequest,
        after: &ApprovalRequest,
        action: &str,
        actor_id: &str,
        reason: Option<String>,
    ) -> Result<()> {
        self.repo
            .insert_transition(&ApprovalTransition {
                id: macro_uuid::generate_uuid_v7(),
                approval_id: after.id,
                action: action.to_string(),
                actor_id: actor_id.to_string(),
                from_user_id: before.assignee_user_id.clone(),
                from_team_id: before.assignee_team_id,
                to_user_id: after.assignee_user_id.clone(),
                to_team_id: after.assignee_team_id,
                reason,
                created_at: Utc::now(),
            })
            .await
    }

    /// Deliver the decision to the runtime callback, best-effort.
    async fn fire_callback(&self, request: &ApprovalRequest) {
        let Some(url) = &request.callback_url else {
            return;
        };
        let payload = serde_json::json!({
            "approval_id": request.id,
            "status": request.status,
            "tool_name": request.tool_name,
            "approved": request.status == ApprovalStatus::Approved,
            "decided_by": request.decided_by,
            "note": request.decision_note,
        });
        let _ = self
            .callback
            .deliver(url, &payload)
            .await
            .inspect_err(|e| tracing::error!(error=?e, approval_id=%request.id, "approval callback delivery failed"));
    }

    async fn notify(&self, request: &ApprovalRequest, recipients: &[String]) {
        if recipients.is_empty() {
            return;
        }
        let _ = self
            .notifier
            .notify_assigned(request, recipients)
            .await
            .inspect_err(|e| tracing::error!(error=?e, approval_id=%request.id, "approval notification failed"));
    }

    async fn recipients_for(&self, request: &ApprovalRequest) -> Vec<String> {
        if let Some(user) = &request.assignee_user_id {
            vec![user.clone()]
        } else if let Some(team) = request.assignee_team_id {
            self.teams.team_members(team).await.unwrap_or_default()
        } else {
            Vec::new()
        }
    }
}

impl<R, P, T, CB, N> ApprovalService for ApprovalServiceImpl<R, P, T, CB, N>
where
    R: ApprovalRepo,
    P: PolicyRepo,
    T: TeamMembershipPort,
    CB: ApprovalCallbackClient,
    N: ApprovalNotifier,
{
    #[tracing::instrument(skip(self), err)]
    async fn evaluate(
        &self,
        org_id: Option<i32>,
        agent_slug: &str,
        tool_name: &str,
    ) -> Result<PolicyDecision> {
        let policies = self.policies.list_policies(org_id).await?;
        let org_decision = Self::winning_policy(&policies, agent_slug, tool_name)
            .map(|p| p.decision)
            .unwrap_or(PolicyDecision::Allow);
        // The floor can only be tightened: the final decision is the
        // stricter of the floor and the org policy.
        Ok(org_decision.max(self.floor.decision(agent_slug, tool_name)))
    }

    #[tracing::instrument(skip(self, request), fields(agent = %request.agent_slug, tool = %request.tool_name), err)]
    async fn gate(
        &self,
        org_id: Option<i32>,
        request: GateRequest,
        actor_id: &str,
    ) -> Result<GateOutcome> {
        if request.summary.trim().is_empty() {
            return Err(ApprovalError::InvalidRequest(
                "a summary is required".to_string(),
            ));
        }
        let decision = self
            .evaluate(org_id, &request.agent_slug, &request.tool_name)
            .await?;
        match decision {
            PolicyDecision::Allow => return Ok(GateOutcome::Allow),
            PolicyDecision::Deny => {
                return Ok(GateOutcome::Deny {
                    reason: "denied by policy".to_string(),
                });
            }
            PolicyDecision::RequireApproval => {}
        }

        // Retry semantics: a prior decision for the same session + tool +
        // arguments authorizes (or refuses) exactly one retry.
        let digest = arguments_digest(&request.arguments);
        if let Some(prior) = self
            .repo
            .find_latest_for_gate(request.session_id, &request.tool_name, &digest)
            .await?
        {
            match prior.status {
                ApprovalStatus::Pending => {
                    return Ok(GateOutcome::Pending {
                        request: Box::new(prior),
                    });
                }
                ApprovalStatus::Approved if prior.consumed_at.is_none() => {
                    if let Some(consumed) = self.repo.consume(prior.id).await? {
                        self.record_transition(&prior, &consumed, "consumed", actor_id, None)
                            .await?;
                        return Ok(GateOutcome::Allow);
                    }
                }
                ApprovalStatus::Denied if prior.consumed_at.is_none() => {
                    if let Some(consumed) = self.repo.consume(prior.id).await? {
                        self.record_transition(&prior, &consumed, "consumed", actor_id, None)
                            .await?;
                        return Ok(GateOutcome::Deny {
                            reason: consumed
                                .decision_note
                                .unwrap_or_else(|| "denied by approver".to_string()),
                        });
                    }
                }
                // Consumed or cancelled: fall through to a fresh request.
                _ => {}
            }
        }

        // Route to the approver named by the winning require_approval
        // policy, when one exists.
        let policies = self.policies.list_policies(org_id).await?;
        let winning = Self::winning_policy(&policies, &request.agent_slug, &request.tool_name);
        let (assignee_user, assignee_team) = match winning {
            Some(p) if p.decision == PolicyDecision::RequireApproval => {
                (p.approver_user_id.clone(), p.approver_team_id)
            }
            _ => (None, None),
        };

        let approval = ApprovalRequest {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            agent_slug: request.agent_slug,
            session_id: request.session_id,
            requester_user_id: request.requester_user_id,
            requester_display: request.requester_display,
            tool_name: request.tool_name,
            arguments: request.arguments,
            arguments_digest: digest,
            summary: request.summary,
            status: ApprovalStatus::Pending,
            assignee_user_id: assignee_user,
            assignee_team_id: assignee_team,
            callback_url: request.callback_url,
            decided_by: None,
            decision_note: None,
            created_at: Utc::now(),
            decided_at: None,
            consumed_at: None,
        };
        self.repo.insert(&approval).await?;
        self.record_transition(&approval, &approval, "routed", actor_id, None)
            .await?;
        let recipients = self.recipients_for(&approval).await;
        self.notify(&approval, &recipients).await;

        Ok(GateOutcome::Pending {
            request: Box::new(approval),
        })
    }

    #[tracing::instrument(skip(self, caller), err)]
    async fn get(&self, caller: &Caller, id: Uuid) -> Result<ApprovalRequest> {
        let request = self.repo.get(id).await?.ok_or(ApprovalError::NotFound)?;
        let visible = match caller {
            Caller::Internal => true,
            Caller::User(user_id) => {
                request.requester_user_id.as_deref() == Some(user_id.as_str())
                    || self.may_act(caller, &request).await?
            }
        };
        if !visible {
            return Err(ApprovalError::NotFound);
        }
        Ok(request)
    }

    #[tracing::instrument(skip(self), err)]
    async fn list_for_user(&self, user_id: &str) -> Result<UserApprovals> {
        let assigned = self
            .repo
            .list(&ApprovalFilter {
                assignee_user_id: Some(user_id.to_string()),
                status: Some(ApprovalStatus::Pending),
                limit: DEFAULT_LIST_LIMIT,
                ..Default::default()
            })
            .await?;

        let mut team_queue = Vec::new();
        for team_id in self.teams.user_teams(user_id).await? {
            let mut items = self
                .repo
                .list(&ApprovalFilter {
                    assignee_team_id: Some(team_id),
                    status: Some(ApprovalStatus::Pending),
                    unassigned_only: true,
                    limit: DEFAULT_LIST_LIMIT,
                    ..Default::default()
                })
                .await?;
            team_queue.append(&mut items);
        }
        team_queue.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        Ok(UserApprovals {
            assigned,
            team_queue,
        })
    }

    #[tracing::instrument(skip(self, caller), err)]
    async fn list_for_team(&self, caller: &Caller, team_id: Uuid) -> Result<Vec<ApprovalRequest>> {
        if let Caller::User(user_id) = caller
            && !self.teams.is_member(user_id, team_id).await?
        {
            return Err(ApprovalError::Forbidden(
                "not a member of this team".to_string(),
            ));
        }
        self.repo
            .list(&ApprovalFilter {
                assignee_team_id: Some(team_id),
                limit: DEFAULT_LIST_LIMIT,
                ..Default::default()
            })
            .await
    }

    #[tracing::instrument(skip(self, caller, note), err)]
    async fn decide(
        &self,
        caller: &Caller,
        id: Uuid,
        approved: bool,
        note: Option<String>,
    ) -> Result<ApprovalRequest> {
        let decider = match caller {
            Caller::User(id) => id.clone(),
            Caller::Internal => {
                return Err(ApprovalError::InvalidRequest(
                    "decisions must name a user".to_string(),
                ));
            }
        };
        let before = self.repo.get(id).await?.ok_or(ApprovalError::NotFound)?;
        if !self.may_act(caller, &before).await? {
            return Err(ApprovalError::Forbidden(
                "only the assignee or team can decide".to_string(),
            ));
        }
        let after = self
            .repo
            .decide(id, approved, &decider, note.as_deref())
            .await?
            .ok_or_else(|| ApprovalError::InvalidStatus("not pending".to_string()))?;
        let action = if approved { "approved" } else { "denied" };
        self.record_transition(&before, &after, action, &decider, note)
            .await?;
        self.fire_callback(&after).await;
        Ok(after)
    }

    #[tracing::instrument(skip(self, caller, reason), err)]
    async fn reassign(
        &self,
        caller: &Caller,
        id: Uuid,
        to_user: Option<String>,
        to_team: Option<Uuid>,
        reason: String,
    ) -> Result<ApprovalRequest> {
        if reason.trim().is_empty() {
            return Err(ApprovalError::InvalidRequest(
                "a reason is required to reassign".to_string(),
            ));
        }
        if to_user.is_none() == to_team.is_none() {
            return Err(ApprovalError::InvalidRequest(
                "exactly one of to_user or to_team is required".to_string(),
            ));
        }
        let before = self.repo.get(id).await?.ok_or(ApprovalError::NotFound)?;
        if before.status != ApprovalStatus::Pending {
            return Err(ApprovalError::InvalidStatus("not pending".to_string()));
        }
        if !self.may_act(caller, &before).await? {
            return Err(ApprovalError::Forbidden(
                "only the assignee or team can reassign".to_string(),
            ));
        }
        let after = self
            .repo
            .reassign(id, to_user.as_deref(), to_team)
            .await?
            .ok_or(ApprovalError::NotFound)?;
        self.record_transition(
            &before,
            &after,
            "reassigned",
            &caller.actor_id(),
            Some(reason),
        )
        .await?;
        let recipients = self.recipients_for(&after).await;
        self.notify(&after, &recipients).await;
        Ok(after)
    }

    #[tracing::instrument(skip(self, caller), err)]
    async fn cancel(&self, caller: &Caller, id: Uuid) -> Result<ApprovalRequest> {
        let before = self.repo.get(id).await?.ok_or(ApprovalError::NotFound)?;
        let allowed = match caller {
            Caller::Internal => true,
            Caller::User(user_id) => {
                before.requester_user_id.as_deref() == Some(user_id.as_str())
                    || self.may_act(caller, &before).await?
            }
        };
        if !allowed {
            return Err(ApprovalError::Forbidden(
                "only the requester, assignee, or team can cancel".to_string(),
            ));
        }
        let after = self
            .repo
            .cancel(id)
            .await?
            .ok_or_else(|| ApprovalError::InvalidStatus("not pending".to_string()))?;
        self.record_transition(&before, &after, "cancelled", &caller.actor_id(), None)
            .await?;
        self.fire_callback(&after).await;
        Ok(after)
    }

    #[tracing::instrument(skip(self, caller), err)]
    async fn transitions(&self, caller: &Caller, id: Uuid) -> Result<Vec<ApprovalTransition>> {
        // Visibility follows `get`.
        self.get(caller, id).await?;
        self.repo.list_transitions(id).await
    }
}
