-- Escalations: agent -> human-expert handoff with configurable routing.
-- See crates/escalations.

CREATE TABLE escalations (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    domain TEXT NOT NULL,
    session_id UUID REFERENCES agent_session_map (session_id),
    requester_user_id TEXT,
    requester_display TEXT NOT NULL,
    source_channel TEXT,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    tags TEXT[] NOT NULL DEFAULT '{}',
    priority TEXT NOT NULL DEFAULT 'normal',
    status TEXT NOT NULL DEFAULT 'open',
    assignee_user_id TEXT,
    assignee_team_id UUID,
    callback_url TEXT,
    resolution TEXT,
    resolved_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    claimed_at TIMESTAMPTZ,
    resolved_at TIMESTAMPTZ
);

-- Personal inbox: items assigned to a user, active first.
CREATE INDEX idx_escalations_assignee_user
    ON escalations (assignee_user_id, status, created_at DESC)
    WHERE assignee_user_id IS NOT NULL;

-- Team queues: unclaimed items by age (SLA visibility).
CREATE INDEX idx_escalations_team_queue
    ON escalations (assignee_team_id, status, created_at)
    WHERE assignee_team_id IS NOT NULL;

-- Org triage / audit listing.
CREATE INDEX idx_escalations_org ON escalations (org_id, created_at DESC);

-- Session linkage (conversation -> escalations lookups).
CREATE INDEX idx_escalations_session
    ON escalations (session_id)
    WHERE session_id IS NOT NULL;

-- Audited state transitions (claim/reassign/resolve history; reassignment
-- reasons feed routing-rule refinement).
CREATE TABLE escalation_transitions (
    id UUID PRIMARY KEY,
    escalation_id UUID NOT NULL REFERENCES escalations (id) ON DELETE CASCADE,
    action TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    from_user_id TEXT,
    from_team_id UUID,
    to_user_id TEXT,
    to_team_id UUID,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_escalation_transitions_escalation
    ON escalation_transitions (escalation_id, created_at);

-- Routing rules: first matching rule (by position) decides the target.
CREATE TABLE escalation_routing_rules (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    domain TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    tags TEXT[] NOT NULL DEFAULT '{}',
    source_channel TEXT,
    min_priority TEXT,
    target_kind TEXT NOT NULL, -- 'user' | 'team_queue' | 'team_round_robin'
    target_user_id TEXT,
    target_team_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_escalation_routing_rules_domain
    ON escalation_routing_rules (org_id, domain, position);

-- Expert routing profiles: availability + coverage. One profile per user
-- (org_id is carried for future multi-tenant partitioning).
CREATE TABLE escalation_experts (
    user_id TEXT PRIMARY KEY,
    org_id INTEGER,
    domains TEXT[] NOT NULL DEFAULT '{}',
    tags TEXT[] NOT NULL DEFAULT '{}',
    available BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Round-robin pointer per rule.
CREATE TABLE escalation_round_robin (
    rule_id UUID PRIMARY KEY
        REFERENCES escalation_routing_rules (id) ON DELETE CASCADE,
    last_user_id TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
