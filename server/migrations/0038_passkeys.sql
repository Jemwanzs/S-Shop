-- Roadmap 132: Face ID & biometric sign-in with passkeys (WebAuthn). The phone or computer checks the face, fingerprint
-- or screen lock itself; S'Shop only ever receives a signed proof — never a camera image or biometric data. Each passkey
-- belongs to the person (their sign-in account, never a linked row) and only holds a public key.
CREATE TABLE passkeys (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- The authenticator's credential id (base64url) — public, used to find the passkey at sign-in.
    credential_id text NOT NULL UNIQUE,
    -- The verified public key and its signature counter (webauthn-rs `Passkey`).
    passkey       jsonb NOT NULL,
    name          text NOT NULL DEFAULT '',
    user_agent    text NOT NULL DEFAULT '',
    created_at    timestamptz NOT NULL DEFAULT now(),
    last_used_at  timestamptz,
    revoked_at    timestamptz,
    revoked_by    uuid REFERENCES users(id) ON DELETE SET NULL,
    revoke_reason text NOT NULL DEFAULT ''
);
CREATE INDEX passkeys_user_idx ON passkeys (user_id) WHERE revoked_at IS NULL;

-- Short-lived, single-use challenges for a registration or a sign-in (deleted when used or expired).
CREATE TABLE passkey_challenges (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind       text NOT NULL CHECK (kind IN ('register', 'login')),
    state      jsonb NOT NULL,
    passkey_id uuid REFERENCES passkeys(id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX passkey_challenges_expiry_idx ON passkey_challenges (expires_at);
