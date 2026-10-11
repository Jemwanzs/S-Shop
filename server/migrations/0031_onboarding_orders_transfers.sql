-- Roadmap 101: onboarding phase 3 — orders and stock transfers (history and outstanding), reconciliation and completion.
ALTER TABLE orders ADD COLUMN import_batch_id uuid REFERENCES import_batches(id) ON DELETE SET NULL;
ALTER TABLE transfers ADD COLUMN import_batch_id uuid REFERENCES import_batches(id) ON DELETE SET NULL;
-- Set once the migration reconciles (no differences, nothing unfinished); imports remain possible afterwards.
ALTER TABLE tenants ADD COLUMN onboarding_completed_at timestamptz;
-- Imported closed orders are history: their revenue lives in Sales history, so they never move (or become a sale) again.
ALTER TABLE orders ADD COLUMN is_legacy boolean NOT NULL DEFAULT false;
