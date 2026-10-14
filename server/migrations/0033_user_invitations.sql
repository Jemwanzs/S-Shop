-- Roadmap 105: users are invited (no administrator-chosen PIN); they set their own PIN through a single-use link.
-- The invitation's state; the link itself is an auth_tokens 'setup' row (hashed, single use, 72 h).
ALTER TABLE users ADD COLUMN invited_at timestamptz;
ALTER TABLE users ADD COLUMN invite_sent_at timestamptz;
-- Result of the last invitation email: sent | failed | skipped (email not configured).
ALTER TABLE users ADD COLUMN invite_email_status text;
ALTER TABLE users ADD COLUMN invite_expires_at timestamptz;
-- The person opened their set-up link (activation in progress).
ALTER TABLE users ADD COLUMN invite_opened_at timestamptz;
-- Offer Quick PIN once after the person sets up their account ("Maybe later" clears it).
ALTER TABLE users ADD COLUMN quick_pin_offer boolean NOT NULL DEFAULT false;

-- The invitation email is logged like every other email.
ALTER TABLE email_log DROP CONSTRAINT email_log_kind_check;
ALTER TABLE email_log ADD CONSTRAINT email_log_kind_check CHECK (kind IN ('access_request_received', 'access_request_ack', 'welcome',
    'access_rejected', 'pin_reset', 'pin_changed', 'request_status', 'login_details', 'receipt', 'user_invite'));
