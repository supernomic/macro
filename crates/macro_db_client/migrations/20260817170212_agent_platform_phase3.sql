-- Agent platform phase 3: skills governance, feedback sidecar, entity graph,
-- lifecycle connectors, ticket mirrors, tenant extensions, training export.

-- 1. Scoped skills (OKF provenance + trust tiers) ---------------------------

CREATE TABLE agent_skills (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    -- 'user' | 'team' | 'org' | 'platform'
    scope TEXT NOT NULL,
    owner_user_id TEXT,
    owner_team_id UUID,
    slug TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    body TEXT NOT NULL,
    -- hermes-style: 'builtin' | 'verified' | 'community' | 'untrusted'
    trust_tier TEXT NOT NULL DEFAULT 'untrusted',
    -- OKF SPEC v0.2-inspired provenance
    okf_type TEXT NOT NULL DEFAULT 'skill',
    okf_sources JSONB NOT NULL DEFAULT '[]'::jsonb,
    okf_generated BOOLEAN NOT NULL DEFAULT FALSE,
    okf_verified BOOLEAN NOT NULL DEFAULT FALSE,
    -- 'draft' | 'active' | 'deprecated' | 'archived'
    okf_status TEXT NOT NULL DEFAULT 'active',
    stale_after TIMESTAMPTZ,
    content_hash TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    archived_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX agent_skills_org_scope_slug_idx
    ON agent_skills (org_id, scope, owner_user_id, owner_team_id, slug)
    NULLS NOT DISTINCT
    WHERE archived_at IS NULL;

CREATE INDEX agent_skills_org_scope_idx ON agent_skills (org_id, scope, updated_at DESC);

CREATE TABLE agent_skill_snapshots (
    id UUID PRIMARY KEY,
    skill_id UUID NOT NULL REFERENCES agent_skills (id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    body TEXT NOT NULL,
    description TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by TEXT NOT NULL
);

CREATE UNIQUE INDEX agent_skill_snapshots_skill_version_idx
    ON agent_skill_snapshots (skill_id, version);

-- 2. Staged skill proposals (inbox review + rollback) ----------------------

CREATE TABLE agent_skill_proposals (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    skill_id UUID REFERENCES agent_skills (id),
    -- 'create' | 'patch' | 'archive'
    kind TEXT NOT NULL,
    slug TEXT NOT NULL,
    -- 'user' | 'team' | 'org' | 'platform'
    target_scope TEXT NOT NULL,
    owner_user_id TEXT,
    owner_team_id UUID,
    proposed_name TEXT NOT NULL,
    proposed_description TEXT NOT NULL,
    proposed_body TEXT NOT NULL,
    diff_summary TEXT NOT NULL,
    evidence JSONB NOT NULL DEFAULT '[]'::jsonb,
    proposer_agent_id TEXT,
    proposer_user_id TEXT,
    -- 'pending' | 'approved' | 'rejected' | 'rolled_back'
    status TEXT NOT NULL DEFAULT 'pending',
    assignee_user_id TEXT,
    assignee_team_id UUID,
    snapshot_id UUID REFERENCES agent_skill_snapshots (id),
    eval_run_id TEXT,
    eval_passed BOOLEAN,
    decided_by TEXT,
    decision_note TEXT,
    decided_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX agent_skill_proposals_inbox_idx
    ON agent_skill_proposals (org_id, status, created_at DESC);
CREATE INDEX agent_skill_proposals_assignee_idx
    ON agent_skill_proposals (assignee_user_id, status)
    WHERE assignee_user_id IS NOT NULL;

CREATE TABLE agent_skill_eval_runs (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    skill_id UUID,
    proposal_id UUID REFERENCES agent_skill_proposals (id),
    composition_id TEXT NOT NULL,
    dataset TEXT NOT NULL,
    passed BOOLEAN NOT NULL,
    score DOUBLE PRECISION,
    report JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX agent_skill_eval_runs_skill_idx
    ON agent_skill_eval_runs (skill_id, created_at DESC);

CREATE TABLE agent_trace_refinements (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    proposal_id UUID REFERENCES agent_skill_proposals (id),
    session_id UUID,
    window_start TIMESTAMPTZ NOT NULL,
    window_end TIMESTAMPTZ NOT NULL,
    evidence_excerpt JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 3. Feedback sidecar + consent (immutable events stay in agent_events) ----

CREATE TABLE agent_message_ratings (
    session_id UUID NOT NULL,
    target_seq BIGINT NOT NULL,
    -- -1 negative, 0 none, 1 positive
    rating SMALLINT NOT NULL,
    note TEXT,
    rated_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (session_id, target_seq, rated_by)
);

CREATE INDEX agent_message_ratings_session_idx
    ON agent_message_ratings (session_id, target_seq);

CREATE TABLE agent_session_consent (
    session_id UUID PRIMARY KEY,
    org_id INTEGER,
    -- 'full' | 'feedback_only' | 'disabled'
    sharing_mode TEXT NOT NULL DEFAULT 'full',
    set_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 4. Entity / knowledge graph ----------------------------------------------

CREATE TABLE entity_graph_nodes (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    -- schema.org-inspired type, e.g. 'Person', 'Device', 'SoftwareApplication'
    node_type TEXT NOT NULL,
    display_name TEXT NOT NULL,
    attributes JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- optional link to a Macro native entity
    native_entity_type TEXT,
    native_entity_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX entity_graph_nodes_org_type_idx
    ON entity_graph_nodes (org_id, node_type, updated_at DESC);
CREATE UNIQUE INDEX entity_graph_nodes_native_idx
    ON entity_graph_nodes (org_id, native_entity_type, native_entity_id)
    WHERE native_entity_type IS NOT NULL;

CREATE TABLE entity_graph_edges (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    from_node_id UUID NOT NULL REFERENCES entity_graph_nodes (id) ON DELETE CASCADE,
    to_node_id UUID NOT NULL REFERENCES entity_graph_nodes (id) ON DELETE CASCADE,
    -- typed relationship, e.g. 'employs', 'owns_device', 'has_account'
    relationship TEXT NOT NULL,
    attributes JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT entity_graph_edges_no_self CHECK (from_node_id <> to_node_id)
);

CREATE UNIQUE INDEX entity_graph_edges_unique_idx
    ON entity_graph_edges (from_node_id, to_node_id, relationship);
CREATE INDEX entity_graph_edges_to_idx
    ON entity_graph_edges (to_node_id, relationship);

CREATE TABLE knowledge_documents (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    -- OKF provenance
    okf_type TEXT NOT NULL DEFAULT 'knowledge',
    okf_sources JSONB NOT NULL DEFAULT '[]'::jsonb,
    okf_generated BOOLEAN NOT NULL DEFAULT TRUE,
    okf_verified BOOLEAN NOT NULL DEFAULT FALSE,
    okf_status TEXT NOT NULL DEFAULT 'draft',
    stale_after TIMESTAMPTZ,
    content_hash TEXT NOT NULL,
    human_authored BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX knowledge_documents_org_slug_idx
    ON knowledge_documents (org_id, slug) NULLS NOT DISTINCT;

-- 5. Lifecycle connector sync state ----------------------------------------

CREATE TABLE lifecycle_connector_accounts (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    -- 'okta' | 'iru' | 'meraki'
    provider TEXT NOT NULL,
    display_name TEXT NOT NULL,
    -- secrets are never stored here; this is the bound host/header identity
    credential_ref TEXT NOT NULL,
    last_synced_at TIMESTAMPTZ,
    last_cursor TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX lifecycle_connector_accounts_org_provider_idx
    ON lifecycle_connector_accounts (org_id, provider, display_name);

CREATE TABLE lifecycle_connector_records (
    id UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES lifecycle_connector_accounts (id) ON DELETE CASCADE,
    external_id TEXT NOT NULL,
    record_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    graph_node_id UUID REFERENCES entity_graph_nodes (id),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX lifecycle_connector_records_ext_idx
    ON lifecycle_connector_records (account_id, record_type, external_id);

-- 6. Ticket mirrors (Zendesk / Jira via foreign_entity pattern) ------------

CREATE TABLE ticket_mirrors (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    -- 'zendesk' | 'jira'
    provider TEXT NOT NULL,
    -- Macro entity that is the source of truth
    native_entity_type TEXT NOT NULL,
    native_entity_id TEXT NOT NULL,
    foreign_id TEXT NOT NULL,
    foreign_url TEXT,
    summary TEXT NOT NULL,
    status TEXT NOT NULL,
    last_mirrored_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    disconnected_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX ticket_mirrors_native_idx
    ON ticket_mirrors (org_id, native_entity_type, native_entity_id, provider)
    WHERE disconnected_at IS NULL;
CREATE UNIQUE INDEX ticket_mirrors_foreign_idx
    ON ticket_mirrors (org_id, provider, foreign_id)
    WHERE disconnected_at IS NULL;

-- 7. Tenant extensions (bb-style) ------------------------------------------

CREATE TABLE tenant_extensions (
    id UUID PRIMARY KEY,
    org_id INTEGER NOT NULL,
    slug TEXT NOT NULL,
    display_name TEXT NOT NULL,
    -- Semver of the extension package
    version TEXT NOT NULL,
    -- Semver range of the host SDK this artifact was stamped against
    sdk_semver TEXT NOT NULL,
    manifest JSONB NOT NULL,
    -- SHA-256 of the artifact; re-tagged releases with a different hash are refused
    artifact_hash TEXT NOT NULL,
    -- 'draft' | 'proposed' | 'active' | 'disabled' | 'rolled_back'
    status TEXT NOT NULL DEFAULT 'draft',
    -- Capability scopes this extension's principal is allowed
    scopes TEXT[] NOT NULL DEFAULT '{}',
    principal_id UUID REFERENCES agent_principals (id),
    activated_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX tenant_extensions_org_slug_idx
    ON tenant_extensions (org_id, slug);

CREATE TABLE tenant_extension_snapshots (
    id UUID PRIMARY KEY,
    extension_id UUID NOT NULL REFERENCES tenant_extensions (id) ON DELETE CASCADE,
    version TEXT NOT NULL,
    manifest JSONB NOT NULL,
    artifact_hash TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by TEXT NOT NULL
);

CREATE TABLE tenant_extension_catalogs (
    org_id INTEGER PRIMARY KEY,
    -- marketplace.json-style catalog for this tenant
    catalog JSONB NOT NULL DEFAULT '{"extensions":[]}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by TEXT NOT NULL
);

-- 8. Training-export job records -------------------------------------------

CREATE TABLE training_export_jobs (
    id UUID PRIMARY KEY,
    org_id INTEGER,
    -- 'model_history' | 'human_transcript' | 'training_export'
    projection TEXT NOT NULL,
    sharing_mode TEXT NOT NULL DEFAULT 'full',
    composition_id TEXT,
    from_occurred_at TIMESTAMPTZ,
    to_occurred_at TIMESTAMPTZ,
    -- 'pending' | 'running' | 'completed' | 'failed'
    status TEXT NOT NULL DEFAULT 'pending',
    row_count BIGINT,
    artifact_uri TEXT,
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

CREATE INDEX training_export_jobs_org_idx
    ON training_export_jobs (org_id, created_at DESC);
