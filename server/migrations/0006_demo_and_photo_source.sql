-- Demo businesses are flagged so they can be reset safely and are never confused with real ones.
ALTER TABLE tenants ADD COLUMN is_demo boolean NOT NULL DEFAULT false;
-- Where a product photo came from (e.g. a Pexels page) and the credit it requires.
ALTER TABLE product_photos ADD COLUMN source text NOT NULL DEFAULT '';
ALTER TABLE product_photos ADD COLUMN attribution text NOT NULL DEFAULT '';
