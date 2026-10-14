-- Roadmap 111: user invitations use their own single-use link kind, valid for 10 minutes
-- (new-business welcome / set-up links keep 'setup', 72 hours).
ALTER TABLE auth_tokens DROP CONSTRAINT IF EXISTS auth_tokens_kind_check;
ALTER TABLE auth_tokens ADD CONSTRAINT auth_tokens_kind_check CHECK (kind IN ('setup', 'reset', 'status', 'invite'));
