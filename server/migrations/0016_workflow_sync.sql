-- Roadmap 49: workflow changes reach pending approvals.
-- Workflow steps get stable ids, each pending request keeps the chain it follows (`steps`) and every decision records
-- the step it approved, so a changed workflow can be reconciled step by step instead of by position.

UPDATE workflows SET levels = (
    SELECT COALESCE(jsonb_agg(CASE WHEN l ? 'id' THEN l ELSE l || jsonb_build_object('id', gen_random_uuid()) END ORDER BY o), '[]'::jsonb)
    FROM jsonb_array_elements(levels) WITH ORDINALITY AS x(l, o)
);

ALTER TABLE approvals
    ADD COLUMN steps     jsonb NOT NULL DEFAULT '[]'::jsonb,
    -- Set when a workflow change needed an exception (step placed before an approval already given, all steps of the
    -- changed workflow already approved); shown on the request.
    ADD COLUMN sync_note text NOT NULL DEFAULT '',
    ADD COLUMN synced_at timestamptz;

-- Requests still pending take the chain they have been following until now.
UPDATE approvals a SET steps = w.levels
FROM workflows w
WHERE w.tenant_id = a.tenant_id AND w.action = a.action AND a.status = 'pending';

-- Their decisions so far remember which step they approved (by the level they were given at).
UPDATE approvals a SET decisions = (
    SELECT COALESCE(jsonb_agg(
        CASE WHEN (d->>'level') ~ '^[0-9]+$' AND (d->>'level')::int BETWEEN 1 AND jsonb_array_length(a.steps)
             THEN d || jsonb_build_object('step_id', a.steps->((d->>'level')::int - 1)->'id')
             ELSE d END ORDER BY o), '[]'::jsonb)
    FROM jsonb_array_elements(a.decisions) WITH ORDINALITY AS x(d, o)
)
WHERE a.status = 'pending' AND jsonb_array_length(a.decisions) > 0;
