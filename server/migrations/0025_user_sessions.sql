-- Roadmap 87: automatic logout after inactivity, enforced by the server. Every staff sign-in (full, Quick PIN, support
-- session) is a session row with its own inactivity limit; only genuine user activity (the app's activity heartbeat
-- and changes the person makes) moves `last_active_at` — background reads, polling and the live stream never do.
CREATE TABLE user_sessions (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id        uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id      uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind           text NOT NULL CHECK (kind IN ('full', 'quick', 'support')),
    created_at     timestamptz NOT NULL DEFAULT now(),
    last_active_at timestamptz NOT NULL DEFAULT now(),
    -- The person's effective limit when the session started (refreshed by every activity heartbeat).
    idle_seconds   int NOT NULL CHECK (idle_seconds BETWEEN 60 AND 86400),
    expires_at     timestamptz NOT NULL,
    ended_at       timestamptz,
    end_reason     text CHECK (end_reason IN ('logout', 'inactivity', 'replaced'))
);
CREATE INDEX user_sessions_open_idx ON user_sessions (user_id) WHERE ended_at IS NULL;

-- Platform defaults and limits (Platform → Security & features).
UPDATE platform_settings SET value = value || '{"idle_default_minutes": 5, "idle_min_minutes": 1, "idle_max_minutes": 120, "warning_seconds": 30, "max_session_hours": 12}'::jsonb
WHERE key = 'security';
