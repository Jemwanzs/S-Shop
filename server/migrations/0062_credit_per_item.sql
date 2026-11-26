-- Roadmap 196: one sale, several credit records. When a sale of several items is left (partly) on credit, the money
-- received pays the cheapest items first and every item (unit) still owing gets its own credit record — linked to the one
-- sale and receipt, independently repaid, recalled or written off. A sale of one item keeps its single credit record
-- for the whole sale (sale_item_id NULL), exactly as before.
ALTER TABLE credit_sales DROP CONSTRAINT credit_sales_sale_id_key;
ALTER TABLE credit_sales
    ADD COLUMN sale_item_id uuid REFERENCES sale_items(id),
    ADD COLUMN unit_no integer,
    -- What this unit costs the customer (its selling price after discounts and its share of points redeemed).
    ADD COLUMN item_price numeric(14,2),
    -- The part of the sale's payment allocated to this unit when the sale was recorded.
    ADD COLUMN paid_at_sale numeric(14,2) NOT NULL DEFAULT 0,
    ADD CONSTRAINT credit_sales_unit_check CHECK ((sale_item_id IS NULL) = (unit_no IS NULL));
CREATE UNIQUE INDEX credit_sales_whole_uq ON credit_sales (sale_id) WHERE sale_item_id IS NULL;
CREATE UNIQUE INDEX credit_sales_unit_uq ON credit_sales (sale_item_id, unit_no) WHERE sale_item_id IS NOT NULL;
CREATE INDEX credit_sales_sale_idx ON credit_sales (sale_id);
