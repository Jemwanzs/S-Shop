-- Roadmap 58–61: onboarding emails with delivery tracking, secure one-time credentials, self-service PIN reset,
-- access-request status for verified applicants.

-- Access requests: estimated users, and a reason the applicant may see (decision_note stays internal).
ALTER TABLE access_requests ADD COLUMN estimated_users int CHECK (estimated_users IS NULL OR estimated_users BETWEEN 1 AND 100000);
ALTER TABLE access_requests ADD COLUMN public_reason text NOT NULL DEFAULT '';
ALTER TABLE access_requests ADD COLUMN admin_user_id uuid REFERENCES users(id) ON DELETE SET NULL;

-- Users: one-time PINs expire and must be replaced at first sign-in; a credential change ends older sessions.
ALTER TABLE users ADD COLUMN must_change_pin boolean NOT NULL DEFAULT false;
ALTER TABLE users ADD COLUMN pin_expires_at timestamptz;
ALTER TABLE users ADD COLUMN sessions_valid_after timestamptz;
ALTER TABLE users ADD COLUMN pin_changed_at timestamptz;

-- Single-use, time-limited links. Only a SHA-256 of the token is stored; the token itself exists only in the email.
--   setup  → welcome email: set the first PIN (72 h)
--   reset  → forgot PIN: set a new PIN (30 min)
--   status → access request applicant: see the request's status (30 min)
CREATE TABLE auth_tokens (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    kind              text NOT NULL CHECK (kind IN ('setup', 'reset', 'status')),
    token_hash        text NOT NULL UNIQUE,
    user_id           uuid REFERENCES users(id) ON DELETE CASCADE,
    access_request_id uuid REFERENCES access_requests(id) ON DELETE CASCADE,
    expires_at        timestamptz NOT NULL,
    used_at           timestamptz,
    revoked_at        timestamptz,
    created_by        uuid REFERENCES users(id) ON DELETE SET NULL,
    ip                text NOT NULL DEFAULT '',
    created_at        timestamptz NOT NULL DEFAULT now(),
    CHECK ((kind = 'status') = (access_request_id IS NOT NULL AND user_id IS NULL))
);
CREATE INDEX auth_tokens_user_idx ON auth_tokens (user_id, kind) WHERE used_at IS NULL AND revoked_at IS NULL;

-- Every email S'Shop sends: what, to whom, and what the provider said. Never the message body, PINs or links.
CREATE TABLE email_log (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    kind              text NOT NULL CHECK (kind IN ('access_request_received', 'access_request_ack', 'welcome', 'access_rejected',
                                                    'pin_reset', 'pin_changed', 'request_status', 'login_details')),
    recipient         text NOT NULL,
    subject           text NOT NULL,
    tenant_id         uuid REFERENCES tenants(id) ON DELETE CASCADE,
    access_request_id uuid REFERENCES access_requests(id) ON DELETE CASCADE,
    user_id           uuid REFERENCES users(id) ON DELETE SET NULL,
    -- queued | sent | delivered | delayed | bounced | complained | failed | skipped (email not configured)
    status            text NOT NULL DEFAULT 'queued'
                      CHECK (status IN ('queued', 'sent', 'delivered', 'delayed', 'bounced', 'complained', 'failed', 'skipped')),
    provider_id       text,
    error             text NOT NULL DEFAULT '',
    attempts          int NOT NULL DEFAULT 0,
    retry_of          uuid REFERENCES email_log(id) ON DELETE SET NULL,
    created_by        uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX email_log_request_idx ON email_log (access_request_id, created_at DESC);
CREATE INDEX email_log_tenant_idx ON email_log (tenant_id, created_at DESC);
CREATE UNIQUE INDEX email_log_provider_uq ON email_log (provider_id) WHERE provider_id IS NOT NULL;

-- Link existing approvals to the administrator they created.
UPDATE access_requests a SET admin_user_id = u.id
FROM users u WHERE a.status = 'approved' AND a.tenant_id = u.tenant_id AND lower(u.email) = lower(a.email) AND a.admin_user_id IS NULL;

-- Administrators approved before this release who have never signed in still hold a temporary PIN that never
-- expired: it keeps working for 72 hours after this release, once, and must then be replaced.
UPDATE users u SET must_change_pin = true, pin_expires_at = now() + interval '72 hours'
FROM access_requests a WHERE a.admin_user_id = u.id AND u.last_login_at IS NULL;
