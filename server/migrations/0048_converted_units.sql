-- Roadmap 170: a product switched from individual items to one shared barcode keeps its units' records. Each unit that
-- was in stock becomes 'converted': its label and history stay (it still scans to the product), and the product's
-- stock — already counted in stock_levels — carries on by quantity. Converted units are not active stock units, so
-- they take no part in the one-active-unit-per-barcode index.
ALTER TABLE stock_items DROP CONSTRAINT stock_items_status_check;
ALTER TABLE stock_items ADD CONSTRAINT stock_items_status_check
    CHECK (status IN ('in_stock','reserved','in_transit','sold','written_off','returned_to_supplier','converted'));
