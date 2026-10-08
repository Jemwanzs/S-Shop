-- Roadmap 70–73: tenant accounts above businesses, controlled platform support access, people working in several
-- businesses of one tenant. Existing data keeps working: every existing business becomes the first business of its own
-- tenant account (same id), and nothing else changes until the platform owner groups businesses or links people.

-- ── 70. Tenant accounts ─────────────────────────────────────────────────────────────────────────────────────────────
-- Platform owner → tenant (the customer account) → businesses (`tenants` rows, unchanged) → branches → users.
CREATE TABLE tenant_accounts (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name             text NOT NULL,
    -- The tenant's main administrator (first approved administrator); contact details come from that user.
    primary_user_id  uuid,
    notes            text NOT NULL DEFAULT '',
    created_at       timestamptz NOT NULL DEFAULT now()
);

INSERT INTO tenant_accounts (id, name, created_at) SELECT id, name, created_at FROM tenants;
ALTER TABLE tenants ADD COLUMN account_id uuid REFERENCES tenant_accounts(id);
UPDATE tenants SET account_id = id;
ALTER TABLE tenants ALTER COLUMN account_id SET NOT NULL;
CREATE INDEX tenants_account_idx ON tenants (account_id);

-- Primary administrator: the administrator created when the access request was approved, else the earliest active
-- Tenant Administrator of the business.
UPDATE tenant_accounts a SET primary_user_id = COALESCE(
    (SELECT r.admin_user_id FROM access_requests r WHERE r.tenant_id = a.id AND r.admin_user_id IS NOT NULL ORDER BY r.decided_at LIMIT 1),
    (SELECT u.id FROM users u JOIN roles ro ON ro.id = u.role_id WHERE u.tenant_id = a.id AND ro.is_system ORDER BY u.created_at LIMIT 1));
ALTER TABLE tenant_accounts ADD CONSTRAINT tenant_accounts_primary_fk FOREIGN KEY (primary_user_id) REFERENCES users(id) ON DELETE SET NULL;

-- ── 71. Platform support access ─────────────────────────────────────────────────────────────────────────────────────
-- The platform owner never signs in as a tenant's administrator and never learns their PIN. Entering a business is a
-- support session: fresh authentication, a reason, a scope, a time limit, the tenant's consent where it asks for it,
-- revocable by either side, and audited from start to end.
CREATE TABLE support_sessions (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id         uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    home_tenant_id  uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    reason          text NOT NULL,
    scope           text NOT NULL CHECK (scope IN ('view', 'full')),
    minutes         int NOT NULL CHECK (minutes BETWEEN 15 AND 480),
    status          text NOT NULL CHECK (status IN ('requested', 'approved', 'active', 'ended', 'denied', 'expired')),
    requested_at    timestamptz NOT NULL DEFAULT now(),
    decided_by      uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_at      timestamptz,
    started_at      timestamptz,
    expires_at      timestamptz,
    ended_at        timestamptz,
    ended_by        uuid REFERENCES users(id) ON DELETE SET NULL,
    end_note        text NOT NULL DEFAULT ''
);
CREATE INDEX support_sessions_tenant_idx ON support_sessions (tenant_id, requested_at DESC);
-- One open request or session per platform user and business.
CREATE UNIQUE INDEX support_sessions_open_uq ON support_sessions (tenant_id, user_id) WHERE status IN ('requested', 'approved', 'active');

-- ── 72. People in several businesses of one tenant ──────────────────────────────────────────────────────────────────
-- A person signs in once (their own user row: email + PIN). Access to another business of the same tenant is a linked
-- user row in that business (its own role, branches, scopes and status) pointing at the sign-in identity, so every
-- business keeps working with its own users exactly as before. Linked rows never sign in by themselves.
ALTER TABLE users ADD COLUMN login_user_id uuid REFERENCES users(id) ON DELETE CASCADE;
DROP INDEX users_email_uq;
CREATE UNIQUE INDEX users_email_uq ON users (lower(email)) WHERE login_user_id IS NULL;
CREATE UNIQUE INDEX users_login_tenant_uq ON users (login_user_id, tenant_id) WHERE login_user_id IS NOT NULL;
