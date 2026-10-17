-- Roadmap 131: a product may have no selling price yet (imported without one). NULL = not set — never treated as 0, so
-- a genuine free product (0) stays distinct. Sale lines keep their own recorded price (sale_items.marked_price stays
-- NOT NULL: a sale always has one).
ALTER TABLE products ALTER COLUMN marked_price DROP NOT NULL;
ALTER TABLE products ALTER COLUMN marked_price DROP DEFAULT;
