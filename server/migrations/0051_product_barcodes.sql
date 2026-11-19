-- Roadmap 173: a product may have several shared barcodes (the same item in different packaging or from different
-- suppliers). `products.barcode` stays its main one; the others are listed here. A barcode belongs to one product of a
-- business; removing it here releases it.
CREATE TABLE product_barcodes (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    product_id  uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    barcode     text NOT NULL,
    created_by  uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, barcode)
);
CREATE INDEX product_barcodes_product_idx ON product_barcodes (product_id);
