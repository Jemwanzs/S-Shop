-- Roadmap 100: onboarding phase 2 — historical sales, credit sales / opening balances, loyalty opening points.
-- Imported records point back to their batch (reports and reconciliation), and imported debts already overdue when
-- imported never trigger automatic overdue reminders.
ALTER TABLE sales ADD COLUMN import_batch_id uuid REFERENCES import_batches(id) ON DELETE SET NULL;
ALTER TABLE credit_sales ADD COLUMN import_batch_id uuid REFERENCES import_batches(id) ON DELETE SET NULL;
ALTER TABLE loyalty_ledger ADD COLUMN import_batch_id uuid REFERENCES import_batches(id) ON DELETE SET NULL;
CREATE INDEX sales_import_batch_idx ON sales (import_batch_id) WHERE import_batch_id IS NOT NULL;
