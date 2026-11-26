-- Roadmap 190: product photos managed from Settings → Website → Products. Shared photos stay one source of truth
-- (product_photos); changes made there wait as a request (new photos staged, never live) until they are applied —
-- at once by someone who may publish, otherwise after approval.
ALTER TABLE product_photos ADD COLUMN hash text;
UPDATE product_photos SET hash = encode(sha256(data), 'hex') WHERE hash IS NULL;
CREATE INDEX product_photos_hash_idx ON product_photos (product_id, hash);

CREATE TABLE product_photo_staging (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    product_id  uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    data        bytea NOT NULL,
    mime        text NOT NULL,
    thumb       bytea,
    thumb_mime  text,
    hash        text NOT NULL,
    upload_ref  uuid,
    created_by  uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX product_photo_staging_ref_uq ON product_photo_staging (product_id, upload_ref) WHERE upload_ref IS NOT NULL;
CREATE INDEX product_photo_staging_tenant_idx ON product_photo_staging (tenant_id, created_at);

CREATE TABLE product_photo_requests (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    product_id       uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    -- The photos as they were when the change was made (ids in order + primary): refused if they changed since.
    base             jsonb NOT NULL,
    -- The proposed photos in order: [{id, staged}], and the primary one.
    proposed         jsonb NOT NULL,
    primary_id       uuid,
    summary          jsonb NOT NULL DEFAULT '{}'::jsonb,
    status           text NOT NULL DEFAULT 'awaiting' CHECK (status IN ('awaiting', 'applied', 'returned', 'rejected', 'withdrawn')),
    requested_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    requested_at     timestamptz NOT NULL DEFAULT now(),
    approval_id      uuid REFERENCES approvals(id) ON DELETE SET NULL,
    decided_by       uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_at       timestamptz,
    decision_comments text NOT NULL DEFAULT ''
);
CREATE UNIQUE INDEX product_photo_requests_awaiting_uq ON product_photo_requests (product_id) WHERE status = 'awaiting';
CREATE INDEX product_photo_requests_tenant_idx ON product_photo_requests (tenant_id, requested_at DESC);

-- Roadmap 191: Edit Sale — every correction of a recorded sale is a request (with the sale as it stood and the proposal)
-- applied only after approval. The sale keeps its id, receipt number, recording time and recorder; each applied edit
-- is a numbered revision with a corrected receipt (the original and earlier receipts stay).
ALTER TABLE sales ADD COLUMN revision integer NOT NULL DEFAULT 0;
ALTER TABLE sales ADD COLUMN effective_time time;

CREATE TABLE sale_edit_requests (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id         uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    sale_id           uuid NOT NULL REFERENCES sales(id),
    branch_id         uuid REFERENCES branches(id),
    status            text NOT NULL DEFAULT 'awaiting' CHECK (status IN ('awaiting', 'applied', 'returned', 'rejected', 'withdrawn')),
    reason            text NOT NULL,
    before            jsonb NOT NULL,
    proposed          jsonb NOT NULL,
    changes           jsonb NOT NULL,
    implications      jsonb NOT NULL DEFAULT '{}'::jsonb,
    after             jsonb,
    revision_no       integer,
    number            text,
    requested_by      uuid REFERENCES users(id) ON DELETE SET NULL,
    requested_at      timestamptz NOT NULL DEFAULT now(),
    approval_id       uuid REFERENCES approvals(id) ON DELETE SET NULL,
    decided_by        uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_at        timestamptz,
    decision_comments text NOT NULL DEFAULT ''
);
CREATE UNIQUE INDEX sale_edit_requests_awaiting_uq ON sale_edit_requests (sale_id) WHERE status = 'awaiting';
CREATE INDEX sale_edit_requests_sale_idx ON sale_edit_requests (sale_id, requested_at DESC);

-- Payment adjustments made by an applied correction are linked to it (payments are never edited or deleted).
ALTER TABLE payments ADD COLUMN sale_edit_id uuid REFERENCES sale_edit_requests(id);

-- Corrected receipts: one per applied revision, linked to the sale (the original receipt is never changed).
ALTER TABLE receipts ADD COLUMN revision_id uuid REFERENCES sale_edit_requests(id);
ALTER TABLE receipts DROP CONSTRAINT receipts_kind_check;
ALTER TABLE receipts ADD CONSTRAINT receipts_kind_check CHECK (kind IN ('original', 'adjustment', 'revision'));
ALTER TABLE receipts ADD CONSTRAINT receipts_revision_check CHECK ((kind = 'revision') = (revision_id IS NOT NULL));
CREATE UNIQUE INDEX receipts_revision_uq ON receipts (revision_id) WHERE kind = 'revision';
