-- Roadmap 99: Business Onboarding & Historical Data Migration Centre (phase 1: products, customers, opening stock,
-- expenses). Each upload is a batch: the workbook as uploaded, every row as read and validated, its status and who
-- uploaded, approved and posted it. Posted records point back to their batch.

CREATE TABLE import_batches (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    dataset        text NOT NULL CHECK (dataset IN ('products','customers','opening_stock','expenses','sales','credit','orders','transfers','loyalty')),
    -- create (new records only) | update (products: existing ones, by product code)
    mode           text NOT NULL DEFAULT 'create' CHECK (mode IN ('create','update')),
    status         text NOT NULL DEFAULT 'validated'
                   CHECK (status IN ('validated','pending_approval','posted','rejected','cancelled','failed')),
    file_name      text NOT NULL,
    file_sha256    text NOT NULL,
    file           bytea NOT NULL,
    -- source header → S'Shop field (as detected or as mapped by the person)
    mapping        jsonb NOT NULL DEFAULT '{}',
    total_rows     int NOT NULL DEFAULT 0,
    valid_rows     int NOT NULL DEFAULT 0,
    warning_rows   int NOT NULL DEFAULT 0,
    invalid_rows   int NOT NULL DEFAULT 0,
    posted_rows    int NOT NULL DEFAULT 0,
    -- Post the valid rows and leave the invalid ones (products and customers only; never stock or money).
    skip_invalid   boolean NOT NULL DEFAULT false,
    summary        jsonb NOT NULL DEFAULT '{}',
    approval_id    uuid,
    uploaded_by    uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    decided_at     timestamptz,
    posted_at      timestamptz,
    error          text
);
CREATE INDEX import_batches_tenant_idx ON import_batches (tenant_id, dataset, created_at DESC);
-- The same workbook can be posted once per dataset.
CREATE UNIQUE INDEX import_batches_posted_file_uq ON import_batches (tenant_id, dataset, file_sha256) WHERE status = 'posted';

CREATE TABLE import_rows (
    id        uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    batch_id  uuid NOT NULL REFERENCES import_batches(id) ON DELETE CASCADE,
    row_no    int NOT NULL,
    -- the row as read (source values) and as understood (normalised values)
    source    jsonb NOT NULL,
    data      jsonb NOT NULL,
    status    text NOT NULL CHECK (status IN ('valid','warning','invalid','posted','skipped')),
    messages  jsonb NOT NULL DEFAULT '[]',
    target_id uuid,
    UNIQUE (batch_id, row_no)
);

-- Records created by an import point back to it (and are reported as imported).
ALTER TABLE products ADD COLUMN import_batch_id uuid REFERENCES import_batches(id) ON DELETE SET NULL;
ALTER TABLE customers ADD COLUMN import_batch_id uuid REFERENCES import_batches(id) ON DELETE SET NULL;
ALTER TABLE expenses ADD COLUMN import_batch_id uuid REFERENCES import_batches(id) ON DELETE SET NULL;
ALTER TABLE stock_movements ADD COLUMN import_batch_id uuid REFERENCES import_batches(id) ON DELETE SET NULL;

-- The migration cutover: records before it are history, balances are as of it, S'Shop runs the business after it.
ALTER TABLE tenants ADD COLUMN migration_cutover date;
