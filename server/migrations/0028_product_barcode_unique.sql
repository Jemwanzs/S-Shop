-- Roadmap 98: a product barcode identifies one product per business, enforced by the database (concurrent saves
-- included). Created only when no business already has two products sharing a barcode, so a deploy never fails on
-- existing data; the app reports such conflicts (Settings → Products → Barcodes) for someone to resolve, and the
-- server retries at every start (catalog::ensure_barcode_index) until the index exists.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM products WHERE barcode IS NOT NULL GROUP BY tenant_id, barcode HAVING count(*) > 1
    ) THEN
        CREATE UNIQUE INDEX IF NOT EXISTS products_barcode_uq ON products (tenant_id, barcode) WHERE barcode IS NOT NULL;
    ELSE
        RAISE NOTICE 'products_barcode_uq not created: duplicate product barcodes exist';
    END IF;
END $$;
