-- Roadmap 188: website publication requests. Someone who may request publication (but not publish) submits the draft for
-- approval; the exact configuration submitted is kept (`snapshot`) with the version it was based on, so approval
-- publishes precisely what was reviewed — never later changes — and an outdated request is detected before publishing.
CREATE TABLE website_publication_requests (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id         uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    approval_id       uuid,
    requested_by      uuid REFERENCES users(id) ON DELETE SET NULL,
    requested_at      timestamptz NOT NULL DEFAULT now(),
    comments          text NOT NULL DEFAULT '',
    snapshot          jsonb NOT NULL,
    base_version      int NOT NULL,
    summary           jsonb NOT NULL DEFAULT '[]'::jsonb,
    total             int NOT NULL DEFAULT 0,
    status            text NOT NULL DEFAULT 'awaiting' CHECK (status IN ('awaiting', 'returned', 'rejected', 'published', 'withdrawn')),
    decided_by        uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_at        timestamptz,
    decision_comments text NOT NULL DEFAULT '',
    published_version int
);
CREATE INDEX website_publication_requests_tenant_idx ON website_publication_requests (tenant_id, requested_at DESC);
-- One request waiting at a time per business (no duplicates, no competing snapshots).
CREATE UNIQUE INDEX website_publication_requests_one_waiting ON website_publication_requests (tenant_id) WHERE status = 'awaiting';
