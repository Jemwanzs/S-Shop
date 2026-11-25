-- Roadmap 182: the confidential Platform Security Audit Trail. Every platform support session (request, decision,
-- start, end, expiry, sign-out) and every change attempted inside one is written here, with the real platform
-- administrator, the business, the reason, the access mode and the outcome. It is kept apart from the businesses'
-- own audit trails, is append-only (no update, delete or truncate — enforced by the database) and has no foreign keys,
-- so nothing removed elsewhere can take a record with it. Only platform administrators read it.
CREATE TABLE platform_audit_log (
    id               bigserial PRIMARY KEY,
    at               timestamptz NOT NULL DEFAULT now(),
    event            text NOT NULL,
    session_id       uuid,
    tenant_id        uuid,
    tenant_name      text NOT NULL DEFAULT '',
    platform_user_id uuid,
    platform_user    text NOT NULL DEFAULT '',
    platform_email   text NOT NULL DEFAULT '',
    scope            text NOT NULL DEFAULT '',
    reason           text NOT NULL DEFAULT '',
    method           text NOT NULL DEFAULT '',
    path             text NOT NULL DEFAULT '',
    status           int,
    detail           jsonb NOT NULL DEFAULT '{}'::jsonb,
    ip               text NOT NULL DEFAULT '',
    user_agent       text NOT NULL DEFAULT ''
);
CREATE INDEX platform_audit_at_idx ON platform_audit_log (at DESC);
CREATE INDEX platform_audit_tenant_idx ON platform_audit_log (tenant_id, at DESC);
CREATE INDEX platform_audit_session_idx ON platform_audit_log (session_id, at);

CREATE FUNCTION platform_audit_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'The platform security audit trail cannot be changed or deleted';
END $$;
CREATE TRIGGER platform_audit_no_change BEFORE UPDATE OR DELETE ON platform_audit_log
    FOR EACH ROW EXECUTE FUNCTION platform_audit_append_only();
CREATE TRIGGER platform_audit_no_truncate BEFORE TRUNCATE ON platform_audit_log
    FOR EACH STATEMENT EXECUTE FUNCTION platform_audit_append_only();

-- What happened in each support session: changes made, changes refused, pages read.
ALTER TABLE support_sessions ADD COLUMN changes int NOT NULL DEFAULT 0,
                             ADD COLUMN refused int NOT NULL DEFAULT 0,
                             ADD COLUMN reads   int NOT NULL DEFAULT 0;

-- The sign-in session that carries a support session: signing out or idling out of it ends the support session too.
ALTER TABLE user_sessions ADD COLUMN support_session_id uuid;

-- A change made in a business by S'Shop support is marked in the business's own audit trail as S'Shop Support —
-- never shown as one of the business's people.
ALTER TABLE audit_log ADD COLUMN support_session_id uuid;

-- Roadmap 183: permissions the business's administrators switched off for the Tenant Administrator role (which keeps
-- full access to everything else). The essential administration permissions can never be listed here.
ALTER TABLE roles ADD COLUMN excluded_permissions text[] NOT NULL DEFAULT '{}';
