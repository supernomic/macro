-- Agent platform foundations: the four shared contracts every agent lane
-- builds on.
--
-- 1. agent_principals / agent_api_tokens — agents as first-class principals
--    with scoped bearer tokens enforced at Macro's API boundary.
-- 2. agent_session_map — mapping between runtime (Flue) conversation ids,
--    Macro ledger sessions, and external threads (Slack/email/channel).
-- 3. agent_events — the append-only, hash-chained session ledger
--    ("model-visible means logged"); doubles as the audit log and the
--    training-data substrate.
-- 4. agent_session_outcomes — queryable projection of terminal outcomes.

-- 1. Agent principals -------------------------------------------------------

CREATE TABLE agent_principals (
    id UUID PRIMARY KEY,
    -- NULL for platform-level agents; otherwise the owning organization.
    org_id INTEGER,
    slug TEXT NOT NULL,
    display_name TEXT NOT NULL,
    -- 'super_agent' | 'domain_agent' | 'workflow_agent' | 'extension'
    kind TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    disabled_at TIMESTAMPTZ
);

-- One slug per org; NULLS NOT DISTINCT so platform-level slugs are unique
-- among themselves too.
CREATE UNIQUE INDEX agent_principals_org_slug_idx
    ON agent_principals (org_id, slug) NULLS NOT DISTINCT;

CREATE TABLE agent_api_tokens (
    id UUID PRIMARY KEY,
    principal_id UUID NOT NULL REFERENCES agent_principals (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    -- SHA-256 digest of the token secret; the secret itself is never stored.
    secret_sha256 BYTEA NOT NULL,
    -- Capability scopes, e.g. 'tool:search', 'ledger:append', 'api:documents:read'.
    scopes TEXT[] NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ
);

CREATE INDEX agent_api_tokens_principal_idx ON agent_api_tokens (principal_id);

-- 2. Session mapping --------------------------------------------------------

CREATE TABLE agent_session_map (
    session_id UUID PRIMARY KEY,
    -- The durable runtime conversation id (Flue conversation).
    runtime_conversation_id TEXT NOT NULL UNIQUE,
    -- 'slack_thread' | 'email_thread' | 'channel_thread' | 'native_chat'
    external_thread_kind TEXT,
    -- e.g. Slack '<channel_id>:<thread_ts>', an email thread id, ...
    external_thread_key TEXT,
    org_id INTEGER,
    agent_principal_id UUID NOT NULL REFERENCES agent_principals (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- An external thread anchors at most one session.
CREATE UNIQUE INDEX agent_session_map_external_thread_idx
    ON agent_session_map (external_thread_kind, external_thread_key)
    WHERE external_thread_kind IS NOT NULL;

CREATE INDEX agent_session_map_principal_idx
    ON agent_session_map (agent_principal_id, created_at DESC);

-- 3. Session ledger ---------------------------------------------------------

CREATE TABLE agent_events (
    session_id UUID NOT NULL,
    -- Monotonic, contiguous, 0-based position within the session. The
    -- (session_id, seq) primary key is the append-race guard: concurrent
    -- writers conflict here and retry against the new chain head.
    seq BIGINT NOT NULL,
    -- Stored discriminant of the typed payload, e.g. 'tool/call'.
    event_type TEXT NOT NULL,
    -- The adjacently-tagged payload JSON ({"type": ..., "data": ...}).
    data JSONB NOT NULL,
    -- 'user' | 'agent' | 'system'
    actor_kind TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    org_id INTEGER,
    -- Producer clock vs. storage clock, kept separate on purpose.
    occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Provenance: seqs of earlier events in this session that produced this
    -- one (derived messages, compaction summaries, ...).
    source_event_seqs BIGINT[] NOT NULL DEFAULT '{}',
    -- Tamper-evident per-session hash chain (Buzz pattern):
    -- hash = SHA-256(prev_hash || session_id || seq || occurred_at ||
    --                actor_kind || actor_id || payload_json)
    prev_hash BYTEA NOT NULL,
    hash BYTEA NOT NULL,

    CONSTRAINT agent_events_pkey PRIMARY KEY (session_id, seq)
);

CREATE INDEX agent_events_org_occurred_idx
    ON agent_events (org_id, occurred_at DESC);
CREATE INDEX agent_events_type_occurred_idx
    ON agent_events (event_type, occurred_at DESC);
CREATE INDEX agent_events_actor_occurred_idx
    ON agent_events (actor_id, occurred_at DESC);

-- 4. Session outcomes -------------------------------------------------------

CREATE TABLE agent_session_outcomes (
    session_id UUID PRIMARY KEY,
    -- 'resolved' | 'unresolved' | 'escalated'
    outcome TEXT NOT NULL,
    summary TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX agent_session_outcomes_outcome_idx
    ON agent_session_outcomes (outcome, updated_at DESC);
