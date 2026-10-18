-- Roadmap 133: the platform-wide registry of seven-digit item barcodes (1000000–9999999). S'Shop issues each code once,
-- from a cryptographically secure random generator, to one business; a code is never issued again — not after the
-- item is sold, returned, written off or relabelled, and not if the business or product is deleted (the row stays,
-- detached). Item codes a business types or scans in this range are registered too, so S'Shop never issues them.
CREATE TABLE barcode_registry (
    code          text PRIMARY KEY CHECK (code ~ '^[1-9][0-9]{6}$'),
    tenant_id     uuid REFERENCES tenants(id) ON DELETE SET NULL,
    product_id    uuid REFERENCES products(id) ON DELETE SET NULL,
    stock_item_id uuid REFERENCES stock_items(id) ON DELETE SET NULL,
    -- generated: issued by S'Shop · entered: typed or scanned by the business (manufacturer / own labels)
    source        text NOT NULL DEFAULT 'generated' CHECK (source IN ('generated', 'entered')),
    -- allocated: issued, label printable, not yet on a received item · assigned: on an item · retired: replaced or
    -- never used — kept so it is never issued again
    status        text NOT NULL DEFAULT 'allocated' CHECK (status IN ('allocated', 'assigned', 'retired')),
    allocated_by  uuid REFERENCES users(id) ON DELETE SET NULL,
    allocated_at  timestamptz NOT NULL DEFAULT now(),
    assigned_at   timestamptz,
    retired_at    timestamptz,
    retire_reason text NOT NULL DEFAULT ''
);
CREATE INDEX barcode_registry_tenant_idx ON barcode_registry (tenant_id, status);
CREATE INDEX barcode_registry_item_idx ON barcode_registry (stock_item_id) WHERE stock_item_id IS NOT NULL;

-- Codes already on items in the seven-digit range are registered (first use wins), so S'Shop never issues them.
INSERT INTO barcode_registry (code, tenant_id, product_id, stock_item_id, source, status, allocated_at, assigned_at)
SELECT DISTINCT ON (barcode) barcode, tenant_id, product_id, id, 'entered', 'assigned', created_at, created_at
  FROM stock_items WHERE barcode ~ '^[1-9][0-9]{6}$'
 ORDER BY barcode, created_at
ON CONFLICT (code) DO NOTHING;

-- Codes an item carried before its label was replaced (roadmap 135): kept so they are never reused and can be traced.
CREATE TABLE stock_item_barcode_history (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    stock_item_id uuid NOT NULL REFERENCES stock_items(id) ON DELETE CASCADE,
    barcode       text NOT NULL,
    replaced_by   uuid REFERENCES users(id) ON DELETE SET NULL,
    reason        text NOT NULL DEFAULT '',
    replaced_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX stock_item_barcode_history_code_idx ON stock_item_barcode_history (barcode);

-- Platform-wide look-ups by code (the existing indexes start with the business).
CREATE INDEX IF NOT EXISTS stock_items_barcode_idx ON stock_items (barcode);
CREATE INDEX IF NOT EXISTS products_barcode_any_idx ON products (barcode) WHERE barcode IS NOT NULL;

-- Barcode permissions (roadmap 133): people who receive stock generate and print labels; people who may write stock
-- off (managers) may also reveal full item barcodes and replace labels; till supervisors may type barcodes at the
-- till when the business allows it.
UPDATE roles SET permissions = permissions || ARRAY['barcodes.generate', 'barcodes.print']
 WHERE NOT ('*' = ANY(permissions)) AND 'stock.add' = ANY(permissions) AND NOT ('barcodes.generate' = ANY(permissions));
UPDATE roles SET permissions = permissions || ARRAY['barcodes.reveal', 'barcodes.replace']
 WHERE NOT ('*' = ANY(permissions)) AND 'stock.write_off' = ANY(permissions) AND NOT ('barcodes.reveal' = ANY(permissions));
UPDATE roles SET permissions = array_append(permissions, 'barcodes.manual_sale')
 WHERE NOT ('*' = ANY(permissions)) AND 'sales.discount_override' = ANY(permissions) AND NOT ('barcodes.manual_sale' = ANY(permissions));

-- Roadmap 136: item barcodes as ordinary screens, reports and exports show them — the last three characters only.
CREATE OR REPLACE FUNCTION mask_item(code text) RETURNS text LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE WHEN code IS NULL THEN NULL WHEN length(code) <= 3 THEN '***' ELSE '****' || right(code, 3) END
$$;

-- Roadmap 137: how each cleared sale line's barcode was captured — scanner (keyboard-style handheld), camera or typed.
-- Informational: a keyboard-style scanner cannot be told apart from fast typing with certainty.
ALTER TABLE sale_items ADD COLUMN scan_source text CHECK (scan_source IN ('scanner', 'camera', 'manual'));
