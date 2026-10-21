-- Roadmap 147: error alerting for the platform owner. One row per distinct error (same source, kind, message shape and
-- place); repeats only count. Messages are shortened and long tokens removed before they are stored; request bodies
-- and headers are never stored.
CREATE TABLE error_events (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    fingerprint    text NOT NULL UNIQUE,
    source         text NOT NULL CHECK (source IN ('server', 'job', 'health', 'test')),
    kind           text NOT NULL,
    message        text NOT NULL,
    method         text NOT NULL DEFAULT '',
    path           text NOT NULL DEFAULT '',
    count          integer NOT NULL DEFAULT 1,
    first_seen     timestamptz NOT NULL DEFAULT now(),
    last_seen      timestamptz NOT NULL DEFAULT now(),
    notified_at    timestamptz,
    resolved_at    timestamptz,
    resolved_by    uuid REFERENCES users(id) ON DELETE SET NULL
);
CREATE INDEX error_events_open_idx ON error_events (last_seen DESC) WHERE resolved_at IS NULL;

-- Each start of the service: a second start of the same deployment is a restart (crash, out of memory), not a deploy.
CREATE TABLE service_starts (
    id             bigserial PRIMARY KEY,
    deployment_id  text NOT NULL,
    started_at     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX service_starts_deployment_idx ON service_starts (deployment_id);

-- Alert emails are recorded in the email log like every other email.
ALTER TABLE email_log DROP CONSTRAINT email_log_kind_check;
ALTER TABLE email_log ADD CONSTRAINT email_log_kind_check CHECK (kind IN ('access_request_received', 'access_request_ack', 'welcome',
    'access_rejected', 'pin_reset', 'pin_changed', 'request_status', 'login_details', 'receipt', 'user_invite', 'error_alert'));
