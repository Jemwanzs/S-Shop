-- Roadmap 172: deleting inventory captured in error. A request is kept per receipt line (one stock movement); after
-- the final approval the stock is reversed by a new movement — the original is never deleted — and an eligible barcode
-- is released for reuse. A unit removed this way is 'voided'.
ALTER TABLE stock_movements DROP CONSTRAINT stock_movements_kind_check;
ALTER TABLE stock_movements ADD CONSTRAINT stock_movements_kind_check CHECK (kind IN (
    'opening','received','sale','order_completion','transfer_out','transfer_in',
    'customer_return','supplier_return','damage','loss','write_off','adjustment',
    'count_variance','sale_reversal','receipt_reversal'));

ALTER TABLE stock_items DROP CONSTRAINT stock_items_status_check;
ALTER TABLE stock_items ADD CONSTRAINT stock_items_status_check
    CHECK (status IN ('in_stock','reserved','in_transit','sold','written_off','returned_to_supplier','converted','voided'));

CREATE TABLE stock_voids (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    branch_id       uuid NOT NULL REFERENCES branches(id),
    product_id      uuid NOT NULL REFERENCES products(id),
    movement_id     uuid NOT NULL REFERENCES stock_movements(id),
    stock_item_id   uuid REFERENCES stock_items(id),
    quantity        int NOT NULL CHECK (quantity > 0),
    reason          text NOT NULL,
    -- pending: waiting for approval · completed: reversed · rejected · withdrawn
    status          text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','completed','rejected','withdrawn')),
    approval_id     uuid,
    reversal_id     uuid REFERENCES stock_movements(id),
    barcode_released boolean NOT NULL DEFAULT false,
    requested_by    uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_by      uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_at      timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now()
);
-- One request waiting per receipt line: no duplicate or conflicting deletions.
CREATE UNIQUE INDEX stock_voids_one_pending ON stock_voids (movement_id) WHERE status = 'pending';
CREATE INDEX stock_voids_tenant_idx ON stock_voids (tenant_id, created_at DESC);

-- Inventory deletion goes through approval by default (one level: the Tenant Administrator); a business may change it.
INSERT INTO workflows (tenant_id, action, enabled, levels)
SELECT t.id, 'stock.void', true, jsonb_build_array(jsonb_build_object('id', gen_random_uuid(), 'approver_type', 'admin'))
  FROM tenants t
 WHERE NOT EXISTS (SELECT 1 FROM workflows w WHERE w.tenant_id = t.id AND w.action = 'stock.void');
