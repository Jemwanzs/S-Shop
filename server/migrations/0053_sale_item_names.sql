-- Roadmap 178: each sale line keeps the product's name and code as they were when it was sold, so a credit sale (and
-- any sale) still shows exactly what was sold after the product is renamed, recoded, repriced or deactivated.
ALTER TABLE sale_items ADD COLUMN product_name text, ADD COLUMN product_code text;

-- Every new line, whichever path records it (till, orders, imports, exchanges), takes the product's name and code now.
CREATE FUNCTION sale_item_snapshot() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.product_name IS NULL OR NEW.product_code IS NULL THEN
        SELECT COALESCE(NEW.product_name, p.name), COALESCE(NEW.product_code, p.code)
          INTO NEW.product_name, NEW.product_code
          FROM products p WHERE p.id = NEW.product_id;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER sale_items_snapshot BEFORE INSERT ON sale_items FOR EACH ROW EXECUTE FUNCTION sale_item_snapshot();

-- Lines recorded before: the name and code at the time of the sale, recovered from the product's edit history (the last
-- edit before the sale, else the first edit after it — its "before"), else the product as it is now.
UPDATE sale_items si SET
    product_name = COALESCE(
        (SELECT a.after->>'name' FROM audit_log a
          WHERE a.entity_type = 'product' AND a.entity_id = si.product_id AND a.action = 'update' AND a.after ? 'name'
            AND a.created_at <= s.created_at ORDER BY a.created_at DESC LIMIT 1),
        (SELECT a.before->>'name' FROM audit_log a
          WHERE a.entity_type = 'product' AND a.entity_id = si.product_id AND a.action = 'update' AND a.before ? 'name'
            AND a.created_at > s.created_at ORDER BY a.created_at ASC LIMIT 1),
        p.name),
    product_code = COALESCE(
        (SELECT a.after->>'code' FROM audit_log a
          WHERE a.entity_type = 'product' AND a.entity_id = si.product_id AND a.action = 'update' AND a.after ? 'code'
            AND a.created_at <= s.created_at ORDER BY a.created_at DESC LIMIT 1),
        (SELECT a.before->>'code' FROM audit_log a
          WHERE a.entity_type = 'product' AND a.entity_id = si.product_id AND a.action = 'update' AND a.before ? 'code'
            AND a.created_at > s.created_at ORDER BY a.created_at ASC LIMIT 1),
        p.code)
FROM sales s, products p
WHERE s.id = si.sale_id AND p.id = si.product_id;
