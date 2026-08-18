CREATE UNIQUE INDEX agent_trace_refinements_org_session_uidx
    ON agent_trace_refinements (org_id, session_id)
    WHERE session_id IS NOT NULL;
