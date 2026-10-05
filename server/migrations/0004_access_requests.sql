-- Roadmap 7: no open signup. Prospective businesses request access; a platform admin approves
-- (which creates the business and its first administrator) or rejects.
CREATE TABLE access_requests (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    business_name  text NOT NULL,
    contact_name   text NOT NULL,
    email          text NOT NULL,
    phone          text NOT NULL,
    location       text NOT NULL DEFAULT '',
    business_type  text NOT NULL DEFAULT '',
    branches       int,
    message        text NOT NULL DEFAULT '',
    status         text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected')),
    tenant_id      uuid REFERENCES tenants(id) ON DELETE SET NULL,
    decided_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_at     timestamptz,
    decision_note  text NOT NULL DEFAULT '',
    email_sent     boolean NOT NULL DEFAULT false,
    ip             text NOT NULL DEFAULT '',
    created_at     timestamptz NOT NULL DEFAULT now()
);
-- One open request per email address.
CREATE UNIQUE INDEX access_requests_pending_email_uq ON access_requests (lower(email)) WHERE status = 'pending';
CREATE INDEX access_requests_status_idx ON access_requests (status, created_at DESC);
