-- Offline POS: a sale made without a connection is queued on the device and sent later. created_at keeps the moment
-- of the sale (so it lands on the right business day); synced_at is when it reached the server (NULL = made online).
ALTER TABLE sales ADD COLUMN synced_at timestamptz;
CREATE INDEX sales_synced_idx ON sales (tenant_id, synced_at) WHERE synced_at IS NOT NULL;
