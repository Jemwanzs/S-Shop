-- Roadmap 83: Quick Login PIN (4–6 digits) on trusted devices. The full email + PIN sign-in stays the default and is
-- required first on every device; the Quick PIN belongs to the person (their sign-in account, never a linked row) and
-- only works on devices they registered. Stored as a salted hash like the full PIN; never shown to anyone.
ALTER TABLE users
    ADD COLUMN quick_pin_hash     text,
    ADD COLUMN quick_pin_set_at   timestamptz,
    ADD COLUMN quick_failed       int NOT NULL DEFAULT 0,
    ADD COLUMN quick_locked_until timestamptz;

CREATE TABLE trusted_devices (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- SHA-256 of the random device secret kept by the browser (the secret itself is never stored).
    token_hash    text NOT NULL UNIQUE,
    name          text NOT NULL DEFAULT '',
    user_agent    text NOT NULL DEFAULT '',
    created_at    timestamptz NOT NULL DEFAULT now(),
    last_used_at  timestamptz,
    expires_at    timestamptz NOT NULL,
    revoked_at    timestamptz,
    revoked_by    uuid REFERENCES users(id) ON DELETE SET NULL,
    revoke_reason text NOT NULL DEFAULT ''
);
CREATE INDEX trusted_devices_user_idx ON trusted_devices (user_id) WHERE revoked_at IS NULL;

-- Platform-wide rules (Platform → Security). Tenants can only narrow them.
INSERT INTO platform_settings (key, value) VALUES
    ('security', '{"quick_pin_enabled": true, "quick_pin_min_length": 4, "quick_session_hours": 12, "max_attempts": 5, "device_days": 90}')
ON CONFLICT (key) DO NOTHING;
