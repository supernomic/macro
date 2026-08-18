-- Approval gates: allow / require_approval / deny policies per agent+tool,
-- pending approval requests routed to approvers, and their audited
-- transitions. Decisions are also recorded as agent_events in the session
-- ledger by the agent runtime; these tables are the actionable queue.

-- Org-configurable tool policies. A built-in floor (in code) can only be
-- tightened by these rows, never loosened.
CREATE TABLE approval_policies (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    -- Agent slug the policy applies to; '*' or a 'prefix*' wildcard.
    agent_slug TEXT NOT NULL,
    -- Tool name the policy applies to; '*' or a 'prefix*' wildcard.
    tool_name TEXT NOT NULL,
    -- 'allow' | 'require_approval' | 'deny'
    decision TEXT NOT NULL,
    -- Who approves when decision = require_approval.
    approver_user_id TEXT,
    approver_team_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX approval_policies_scope_idx
    ON approval_policies (org_id, agent_slug, tool_name) NULLS NOT DISTINCT;

-- One pending/decided approval request per gated tool call.
CREATE TABLE approval_requests (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    agent_slug TEXT NOT NULL,
    session_id UUID,
    requester_user_id TEXT,
    requester_display TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    arguments JSONB NOT NULL,
    -- SHA-256 hex of the canonical arguments; used to match an approved
    -- request when the agent retries the same call.
    arguments_digest TEXT NOT NULL,
    summary TEXT NOT NULL,
    -- 'pending' | 'approved' | 'denied' | 'cancelled'
    status TEXT NOT NULL,
    assignee_user_id TEXT,
    assignee_team_id UUID,
    callback_url TEXT,
    decided_by TEXT,
    decision_note TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    decided_at TIMESTAMPTZ,
    -- Set when an approved/denied request has been consumed by a retry of
    -- the gated call; a consumed approval cannot authorize a second call.
    consumed_at TIMESTAMPTZ
);

CREATE INDEX approval_requests_gate_idx
    ON approval_requests (session_id, tool_name, arguments_digest, created_at DESC);
CREATE INDEX approval_requests_assignee_user_idx
    ON approval_requests (assignee_user_id, status)
    WHERE assignee_user_id IS NOT NULL;
CREATE INDEX approval_requests_assignee_team_idx
    ON approval_requests (assignee_team_id, status)
    WHERE assignee_team_id IS NOT NULL;
CREATE INDEX approval_requests_org_created_idx
    ON approval_requests (org_id, created_at DESC);

-- Audited state transitions (routed, decided, reassigned, cancelled).
CREATE TABLE approval_transitions (
    id UUID PRIMARY KEY,
    approval_id UUID NOT NULL REFERENCES approval_requests (id),
    action TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    from_user_id TEXT,
    from_team_id UUID,
    to_user_id TEXT,
    to_team_id UUID,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX approval_transitions_approval_idx
    ON approval_transitions (approval_id, created_at);
