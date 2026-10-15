-- Roadmap 115: editable sale dates. A sale (and each of its lines) may carry an earlier effective date chosen by
-- someone allowed to backdate; the real entry time (created_at) never changes. The business date of the sale, its
-- payments (except M-Pesa confirmations), its credit and its stock movements follows the effective date.
ALTER TABLE sales ADD COLUMN effective_date date;
ALTER TABLE sale_items ADD COLUMN sale_date date;            -- a line's own date when it differs from the sale's
ALTER TABLE stock_movements ADD COLUMN effective_date date;
ALTER TABLE payments ADD COLUMN effective_date date;
ALTER TABLE credit_sales ADD COLUMN effective_date date;

CREATE FUNCTION set_effective_business_date() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.business_date := COALESCE(NEW.effective_date, business_date_of(NEW.created_at, NEW.tenant_id, NEW.branch_id));
    RETURN NEW;
END $$;

DROP TRIGGER sales_business_date ON sales;
DROP TRIGGER payments_business_date ON payments;
DROP TRIGGER credit_sales_business_date ON credit_sales;
DROP TRIGGER stock_movements_business_date ON stock_movements;
CREATE TRIGGER sales_business_date BEFORE INSERT OR UPDATE OF created_at, effective_date ON sales
    FOR EACH ROW EXECUTE FUNCTION set_effective_business_date();
CREATE TRIGGER payments_business_date BEFORE INSERT OR UPDATE OF created_at, effective_date ON payments
    FOR EACH ROW EXECUTE FUNCTION set_effective_business_date();
CREATE TRIGGER credit_sales_business_date BEFORE INSERT OR UPDATE OF created_at, effective_date ON credit_sales
    FOR EACH ROW EXECUTE FUNCTION set_effective_business_date();
CREATE TRIGGER stock_movements_business_date BEFORE INSERT OR UPDATE OF created_at, effective_date ON stock_movements
    FOR EACH ROW EXECUTE FUNCTION set_effective_business_date();

-- Backdating is a supervisor's right by default: the roles that may approve discount overrides get it.
UPDATE roles SET permissions = array_append(permissions, 'sales.backdate')
 WHERE NOT ('*' = ANY(permissions)) AND NOT ('sales.backdate' = ANY(permissions))
   AND 'sales.discount_override' = ANY(permissions);
