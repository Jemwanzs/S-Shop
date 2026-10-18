-- Roadmap 142: stock balances uploaded from Excel become ordinary count adjustments, traceable to their upload
-- (uploader, file, approval) and carrying the balance's effective date; processing time stays in created_at.
ALTER TABLE stock_adjustments
    ADD COLUMN import_batch_id uuid REFERENCES import_batches(id) ON DELETE SET NULL,
    ADD COLUMN effective_date  date;
CREATE INDEX stock_adjustments_import_idx ON stock_adjustments (import_batch_id) WHERE import_batch_id IS NOT NULL;

-- Stock balance uploads use the onboarding import engine (a new dataset).
ALTER TABLE import_batches DROP CONSTRAINT IF EXISTS import_batches_dataset_check;
ALTER TABLE import_batches ADD CONSTRAINT import_batches_dataset_check
    CHECK (dataset IN ('products','customers','opening_stock','expenses','sales','credit','orders','transfers','loyalty','stock_balances'));
