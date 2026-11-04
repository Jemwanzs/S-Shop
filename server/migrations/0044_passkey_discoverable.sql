-- Roadmap 155: sign-in with a passkey the device itself offers. The browser's saved record of "which passkey this device
-- set up" is lost when storage is cleared and is not shared between Safari and the iPhone home-screen app, so the
-- sign-in challenge may be issued before anyone is known. The person is identified from the signed answer.
ALTER TABLE passkey_challenges ALTER COLUMN user_id DROP NOT NULL;
ALTER TABLE passkey_challenges DROP CONSTRAINT passkey_challenges_kind_check;
ALTER TABLE passkey_challenges ADD CONSTRAINT passkey_challenges_kind_check CHECK (kind IN ('register', 'login', 'discover'));
