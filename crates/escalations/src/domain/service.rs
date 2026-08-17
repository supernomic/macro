//! Escalation domain service: routing, claim/reassign/resolve semantics,
//! and resume-on-reply callbacks.

#[cfg(test)]
mod test;

use chrono::Utc;
use macro_uuid::Uuid;

use super::model::{
    Escalation, EscalationError, EscalationStatus, EscalationTransition, NewEscalation, Result,
    RouteTarget,
};
use super::ports::{
    EscalationCallbackClient, EscalationFilter, EscalationNotifier, EscalationRepo, RoutingRepo,
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

/// Personal inbox view: items assigned to the user plus items they can
/// claim from their teams' queues.
#[derive(Debug, Clone)]
pub struct UserEscalations {
    /// Items assigned to (or claimed by) the user, non-terminal.
    pub assigned: Vec<Escalation>,
    /// Unclaimed items on the user's teams' queues.
    pub claimable: Vec<Escalation>,
}

/// Domain service exposed to inbound adapters.
pub trait EscalationService: Send + Sync + 'static {
    /// Create an escalation, applying routing rules. `actor_id` names the
    /// creating agent principal (or `system`).
    fn create(
        &self,
        org_id: Option<i32>,
        request: NewEscalation,
        actor_id: &str,
    ) -> impl Future<Output = Result<Escalation>> + Send;

    /// Fetch one escalation visible to the caller.
    fn get(&self, caller: &Caller, id: Uuid) -> impl Future<Output = Result<Escalation>> + Send;

    /// The caller's personal inbox view.
    fn list_for_user(&self, user_id: &str) -> impl Future<Output = Result<UserEscalations>> + Send;

    /// Everything on one team's queue (member-only): unclaimed first.
    fn list_for_team(
        &self,
        caller: &Caller,
        team_id: Uuid,
    ) -> impl Future<Output = Result<Vec<Escalation>>> + Send;

    /// Claim an open escalation (first claim wins).
    fn claim(&self, caller: &Caller, id: Uuid) -> impl Future<Output = Result<Escalation>> + Send;

    /// Reassign to another user or team queue, with a required reason.
    fn reassign(
        &self,
        caller: &Caller,
        id: Uuid,
        to_user: Option<String>,
        to_team: Option<Uuid>,
        reason: String,
    ) -> impl Future<Output = Result<Escalation>> + Send;

    /// Resolve with an expert answer; triggers the runtime callback.
    fn resolve(
        &self,
        caller: &Caller,
        id: Uuid,
        resolution: String,
    ) -> impl Future<Output = Result<Escalation>> + Send;

    /// Cancel without a resolution; triggers the runtime callback.
    fn cancel(&self, caller: &Caller, id: Uuid) -> impl Future<Output = Result<Escalation>> + Send;

    /// An escalation's audited transition history.
    fn transitions(
        &self,
        caller: &Caller,
        id: Uuid,
    ) -> impl Future<Output = Result<Vec<EscalationTransition>>> + Send;
}

/// Concrete escalation service over the storage and delivery ports.
#[derive(Debug, Clone)]
pub struct EscalationServiceImpl<R, RT, T, CB, N> {
    repo: R,
    routing: RT,
    teams: T,
    callback: CB,
    notifier: N,
}

impl<R, RT, T, CB, N> EscalationServiceImpl<R, RT, T, CB, N>
where
    R: EscalationRepo,
    RT: RoutingRepo,
    T: TeamMembershipPort,
    CB: EscalationCallbackClient,
    N: EscalationNotifier,
{
    /// Build the service over its ports.
    pub fn new(repo: R, routing: RT, teams: T, callback: CB, notifier: N) -> Self {
        Self {
            repo,
            routing,
            teams,
            callback,
            notifier,
        }
    }

    /// Pick the next available round-robin assignee among team members.
    /// Falls back to `None` (team-queue behavior) when no member has an
    /// available, domain-covering expert profile.
    async fn round_robin_pick(
        &self,
        rule_id: Uuid,
        org_id: Option<i32>,
        team_id: Uuid,
        domain: &str,
    ) -> Result<Option<String>> {
        let mut members = self.teams.team_members(team_id).await?;
        members.sort();
        let mut eligible = Vec::new();
        for member in &members {
            match self.routing.get_expert(org_id, member).await? {
                Some(profile) => {
                    let covers =
                        profile.domains.is_empty() || profile.domains.iter().any(|d| d == domain);
                    if profile.available && covers {
                        eligible.push(member.clone());
                    }
                }
                // No profile yet: treated as available (profiles are an
                // opt-in refinement, not a participation requirement).
                None => eligible.push(member.clone()),
            }
        }
        if eligible.is_empty() {
            return Ok(None);
        }
        let last = self.routing.round_robin_last(rule_id).await?;
        let next = match last.and_then(|l| eligible.iter().position(|m| *m == l)) {
            Some(pos) => eligible[(pos + 1) % eligible.len()].clone(),
            None => eligible[0].clone(),
        };
        self.routing.set_round_robin_last(rule_id, &next).await?;
        Ok(Some(next))
    }

    /// Whether the caller may act on this escalation (assignee, member of
    /// the assigned team, or internal).
    async fn may_act(&self, caller: &Caller, escalation: &Escalation) -> Result<bool> {
        let user_id = match caller {
            Caller::Internal => return Ok(true),
            Caller::User(id) => id,
        };
        if escalation.assignee_user_id.as_deref() == Some(user_id.as_str()) {
            return Ok(true);
        }
        if let Some(team_id) = escalation.assignee_team_id {
            return self.teams.is_member(user_id, team_id).await;
        }
        Ok(false)
    }

    async fn record_transition(
        &self,
        escalation_before: &Escalation,
        escalation_after: &Escalation,
        action: &str,
        actor_id: &str,
        reason: Option<String>,
    ) -> Result<()> {
        self.repo
            .insert_transition(&EscalationTransition {
                id: macro_uuid::generate_uuid_v7(),
                escalation_id: escalation_after.id,
                action: action.to_string(),
                actor_id: actor_id.to_string(),
                from_user_id: escalation_before.assignee_user_id.clone(),
                from_team_id: escalation_before.assignee_team_id,
                to_user_id: escalation_after.assignee_user_id.clone(),
                to_team_id: escalation_after.assignee_team_id,
                reason,
                created_at: Utc::now(),
            })
            .await
    }

    /// Deliver the terminal state to the runtime callback, best-effort.
    async fn fire_callback(&self, escalation: &Escalation) {
        let Some(url) = &escalation.callback_url else {
            return;
        };
        let payload = serde_json::json!({
            "escalation_id": escalation.id,
            "status": escalation.status,
            "domain": escalation.domain,
            "resolution": escalation.resolution,
            "resolved_by": escalation.resolved_by,
        });
        let _ = self
            .callback
            .deliver(url, &payload)
            .await
            .inspect_err(|e| tracing::error!(error=?e, escalation_id=%escalation.id, "escalation callback delivery failed"));
    }

    async fn notify(&self, escalation: &Escalation, recipients: &[String]) {
        if recipients.is_empty() {
            return;
        }
        let _ = self
            .notifier
            .notify_assigned(escalation, recipients)
            .await
            .inspect_err(|e| tracing::error!(error=?e, escalation_id=%escalation.id, "escalation notification failed"));
    }
}

impl<R, RT, T, CB, N> EscalationService for EscalationServiceImpl<R, RT, T, CB, N>
where
    R: EscalationRepo,
    RT: RoutingRepo,
    T: TeamMembershipPort,
    CB: EscalationCallbackClient,
    N: EscalationNotifier,
{
    #[tracing::instrument(skip(self, request), fields(domain = %request.domain), err)]
    async fn create(
        &self,
        org_id: Option<i32>,
        request: NewEscalation,
        actor_id: &str,
    ) -> Result<Escalation> {
        if request.title.trim().is_empty() || request.summary.trim().is_empty() {
            return Err(EscalationError::InvalidRequest(
                "title and summary are required".to_string(),
            ));
        }

        let rules = self.routing.list_rules(org_id, &request.domain).await?;
        let matched = rules.iter().find(|r| r.matches(&request));

        let (assignee_user, assignee_team) = match matched.map(|r| (&r.target, r.id)) {
            Some((RouteTarget::User { user_id }, _)) => (Some(user_id.clone()), None),
            Some((RouteTarget::TeamQueue { team_id }, _)) => (None, Some(*team_id)),
            Some((RouteTarget::TeamRoundRobin { team_id }, rule_id)) => {
                let picked = self
                    .round_robin_pick(rule_id, org_id, *team_id, &request.domain)
                    .await?;
                (picked, Some(*team_id))
            }
            None => (None, None),
        };

        let escalation = Escalation {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            domain: request.domain,
            session_id: request.session_id,
            requester_user_id: request.requester_user_id,
            requester_display: request.requester_display,
            source_channel: request.source_channel,
            title: request.title,
            summary: request.summary,
            tags: request.tags,
            priority: request.priority,
            status: EscalationStatus::Open,
            assignee_user_id: assignee_user,
            assignee_team_id: assignee_team,
            callback_url: request.callback_url,
            resolution: None,
            resolved_by: None,
            created_at: Utc::now(),
            claimed_at: None,
            resolved_at: None,
        };
        self.repo.insert(&escalation).await?;
        self.record_transition(&escalation, &escalation, "routed", actor_id, None)
            .await?;

        // Notify the direct assignee, or the whole team for queue items.
        let recipients = if let Some(user) = &escalation.assignee_user_id {
            vec![user.clone()]
        } else if let Some(team) = escalation.assignee_team_id {
            self.teams.team_members(team).await.unwrap_or_default()
        } else {
            Vec::new()
        };
        self.notify(&escalation, &recipients).await;

        Ok(escalation)
    }

    #[tracing::instrument(skip(self, caller), err)]
    async fn get(&self, caller: &Caller, id: Uuid) -> Result<Escalation> {
        let escalation = self.repo.get(id).await?.ok_or(EscalationError::NotFound)?;
        let visible = match caller {
            Caller::Internal => true,
            Caller::User(user_id) => {
                escalation.requester_user_id.as_deref() == Some(user_id.as_str())
                    || self.may_act(caller, &escalation).await?
            }
        };
        if !visible {
            return Err(EscalationError::NotFound);
        }
        Ok(escalation)
    }

    #[tracing::instrument(skip(self), err)]
    async fn list_for_user(&self, user_id: &str) -> Result<UserEscalations> {
        let assigned = self
            .repo
            .list(&EscalationFilter {
                assignee_user_id: Some(user_id.to_string()),
                limit: DEFAULT_LIST_LIMIT,
                ..Default::default()
            })
            .await?
            .into_iter()
            .filter(|e| matches!(e.status, EscalationStatus::Open | EscalationStatus::Claimed))
            .collect();

        let mut claimable = Vec::new();
        for team_id in self.teams.user_teams(user_id).await? {
            let mut items = self
                .repo
                .list(&EscalationFilter {
                    assignee_team_id: Some(team_id),
                    status: Some(EscalationStatus::Open),
                    unclaimed_only: true,
                    limit: DEFAULT_LIST_LIMIT,
                    ..Default::default()
                })
                .await?;
            claimable.append(&mut items);
        }
        claimable.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        Ok(UserEscalations {
            assigned,
            claimable,
        })
    }

    #[tracing::instrument(skip(self, caller), err)]
    async fn list_for_team(&self, caller: &Caller, team_id: Uuid) -> Result<Vec<Escalation>> {
        if let Caller::User(user_id) = caller
            && !self.teams.is_member(user_id, team_id).await?
        {
            return Err(EscalationError::Forbidden(
                "not a member of this team".to_string(),
            ));
        }
        self.repo
            .list(&EscalationFilter {
                assignee_team_id: Some(team_id),
                limit: DEFAULT_LIST_LIMIT,
                ..Default::default()
            })
            .await
    }

    #[tracing::instrument(skip(self, caller), err)]
    async fn claim(&self, caller: &Caller, id: Uuid) -> Result<Escalation> {
        let user_id = match caller {
            Caller::User(id) => id.clone(),
            Caller::Internal => {
                return Err(EscalationError::InvalidRequest(
                    "claims must name a user".to_string(),
                ));
            }
        };
        let before = self.repo.get(id).await?.ok_or(EscalationError::NotFound)?;
        // A direct assignment to someone else cannot be claimed away; a
        // team queue item requires membership.
        if let Some(assignee) = &before.assignee_user_id {
            if assignee != &user_id {
                return Err(EscalationError::Forbidden(
                    "assigned to someone else".to_string(),
                ));
            }
        } else if let Some(team_id) = before.assignee_team_id
            && !self.teams.is_member(&user_id, team_id).await?
        {
            return Err(EscalationError::Forbidden(
                "not a member of the assigned team".to_string(),
            ));
        }
        let after = self
            .repo
            .claim(id, &user_id)
            .await?
            .ok_or_else(|| EscalationError::InvalidStatus("not open".to_string()))?;
        self.record_transition(&before, &after, "claimed", &user_id, None)
            .await?;
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
    ) -> Result<Escalation> {
        if reason.trim().is_empty() {
            return Err(EscalationError::InvalidRequest(
                "a reason is required to reassign".to_string(),
            ));
        }
        if to_user.is_none() == to_team.is_none() {
            return Err(EscalationError::InvalidRequest(
                "exactly one of to_user or to_team is required".to_string(),
            ));
        }
        let before = self.repo.get(id).await?.ok_or(EscalationError::NotFound)?;
        if !matches!(
            before.status,
            EscalationStatus::Open | EscalationStatus::Claimed
        ) {
            return Err(EscalationError::InvalidStatus(
                "already terminal".to_string(),
            ));
        }
        if !self.may_act(caller, &before).await? {
            return Err(EscalationError::Forbidden(
                "only the assignee or team can reassign".to_string(),
            ));
        }
        let after = self
            .repo
            .reassign(id, to_user.as_deref(), to_team)
            .await?
            .ok_or(EscalationError::NotFound)?;
        self.record_transition(
            &before,
            &after,
            "reassigned",
            &caller.actor_id(),
            Some(reason),
        )
        .await?;

        let recipients = if let Some(user) = &after.assignee_user_id {
            vec![user.clone()]
        } else if let Some(team) = after.assignee_team_id {
            self.teams.team_members(team).await.unwrap_or_default()
        } else {
            Vec::new()
        };
        self.notify(&after, &recipients).await;
        Ok(after)
    }

    #[tracing::instrument(skip(self, caller, resolution), err)]
    async fn resolve(&self, caller: &Caller, id: Uuid, resolution: String) -> Result<Escalation> {
        if resolution.trim().is_empty() {
            return Err(EscalationError::InvalidRequest(
                "a resolution is required".to_string(),
            ));
        }
        let before = self.repo.get(id).await?.ok_or(EscalationError::NotFound)?;
        if !self.may_act(caller, &before).await? {
            return Err(EscalationError::Forbidden(
                "only the assignee or team can resolve".to_string(),
            ));
        }
        let actor = caller.actor_id();
        let after = self
            .repo
            .resolve(id, &resolution, &actor)
            .await?
            .ok_or_else(|| EscalationError::InvalidStatus("already terminal".to_string()))?;
        self.record_transition(&before, &after, "resolved", &actor, None)
            .await?;
        self.fire_callback(&after).await;
        Ok(after)
    }

    #[tracing::instrument(skip(self, caller), err)]
    async fn cancel(&self, caller: &Caller, id: Uuid) -> Result<Escalation> {
        let before = self.repo.get(id).await?.ok_or(EscalationError::NotFound)?;
        let allowed = match caller {
            Caller::Internal => true,
            Caller::User(user_id) => {
                before.requester_user_id.as_deref() == Some(user_id.as_str())
                    || self.may_act(caller, &before).await?
            }
        };
        if !allowed {
            return Err(EscalationError::Forbidden(
                "only the requester, assignee, or team can cancel".to_string(),
            ));
        }
        let after = self
            .repo
            .cancel(id)
            .await?
            .ok_or_else(|| EscalationError::InvalidStatus("already terminal".to_string()))?;
        self.record_transition(&before, &after, "cancelled", &caller.actor_id(), None)
            .await?;
        self.fire_callback(&after).await;
        Ok(after)
    }

    #[tracing::instrument(skip(self, caller), err)]
    async fn transitions(&self, caller: &Caller, id: Uuid) -> Result<Vec<EscalationTransition>> {
        // Visibility follows `get`.
        self.get(caller, id).await?;
        self.repo.list_transitions(id).await
    }
}
