-- Roadmap 65–67: one compact digital receipt for every channel, stored as an immutable snapshot; adjustment receipts
-- after returns / exchanges; loyalty points that could not be taken back are recorded, never silently dropped.

-- Images a receipt shows (the business logo as it was when the receipt was issued), stored once by content hash.
CREATE TABLE receipt_assets (
    hash       text PRIMARY KEY,
    mime       text NOT NULL,
    data       bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- Issued receipts: the original receipt of a sale, and an adjustment receipt per return / exchange / cancellation.
-- `snapshot` is everything the receipt shows, frozen at issue time, so later changes to products, logo, prices or the
-- sale owner never alter a receipt already issued.
CREATE TABLE receipts (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    sale_id     uuid NOT NULL REFERENCES sales(id),
    kind        text NOT NULL CHECK (kind IN ('original', 'adjustment')),
    number      text NOT NULL,
    return_id   uuid REFERENCES sale_returns(id),
    snapshot    jsonb NOT NULL,
    -- Secure share link (/r/{token}): unguessable; the receipt is readable by whoever has the link.
    share_token uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    created_by  uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CHECK ((kind = 'adjustment') = (return_id IS NOT NULL))
);
CREATE UNIQUE INDEX receipts_original_uq ON receipts (sale_id) WHERE kind = 'original';
CREATE UNIQUE INDEX receipts_adjustment_uq ON receipts (return_id) WHERE kind = 'adjustment';
CREATE INDEX receipts_sale_idx ON receipts (sale_id, created_at);

-- Receipts can be emailed (delivery tracked like every other email).
ALTER TABLE email_log DROP CONSTRAINT email_log_kind_check;
ALTER TABLE email_log ADD CONSTRAINT email_log_kind_check CHECK (kind IN ('access_request_received', 'access_request_ack', 'welcome',
    'access_rejected', 'pin_reset', 'pin_changed', 'request_status', 'login_details', 'receipt'));

-- Points that could not be reversed because the customer had already redeemed them (an outstanding liability).
ALTER TABLE sale_returns ADD COLUMN points_unrecovered bigint NOT NULL DEFAULT 0;
