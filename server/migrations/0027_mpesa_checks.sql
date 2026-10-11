-- Roadmap 96: each M-Pesa account shows its checks separately — OAuth (credentials), STK request accepted
-- (verified_at) and a test payment confirmed (from its latest test prompt).
ALTER TABLE mpesa_accounts ADD COLUMN oauth_ok_at timestamptz;
