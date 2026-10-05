-- Roadmap 16: working days, trading hours and the business (trading) date.
--
-- Every trading record keeps its true timestamp (created_at) and also stores the business date it belongs to.
-- With hours 06:00 → 02:00 a sale at 01:30 on Tuesday belongs to Monday. The date is a snapshot taken when the
-- record is written, so changing hours later never moves past transactions between days.

-- Per-branch override of the business-wide hours (NULL = follow Settings → Workspace), and the resulting shift:
-- minutes after midnight that still belong to the previous business day (02:00 → 120; midnight close → 0).
ALTER TABLE branches ADD COLUMN hours jsonb;
-- Branches always hold their effective shift (own hours, else the business hours); the tenant's shift applies to
-- records that belong to no branch (customers, loyalty).
ALTER TABLE branches ADD COLUMN day_shift_minutes int NOT NULL DEFAULT 0 CHECK (day_shift_minutes BETWEEN 0 AND 1439);
ALTER TABLE tenants ADD COLUMN day_shift_minutes int NOT NULL DEFAULT 0 CHECK (day_shift_minutes BETWEEN 0 AND 1439);

CREATE FUNCTION business_date_of(ts timestamptz, tenant uuid, branch uuid) RETURNS date
LANGUAGE sql STABLE AS $$
    SELECT ((ts AT TIME ZONE t.timezone) - make_interval(mins => COALESCE(b.day_shift_minutes, t.day_shift_minutes)))::date
    FROM tenants t LEFT JOIN branches b ON b.id = branch
    WHERE t.id = tenant
$$;

ALTER TABLE sales ADD COLUMN business_date date;
ALTER TABLE orders ADD COLUMN business_date date;
ALTER TABLE payments ADD COLUMN business_date date;
ALTER TABLE credit_sales ADD COLUMN business_date date;
ALTER TABLE stock_movements ADD COLUMN business_date date;
ALTER TABLE sale_returns ADD COLUMN business_date date;

-- Existing records: midnight-to-midnight (no shift was configured before).
UPDATE sales SET business_date = business_date_of(created_at, tenant_id, branch_id);
UPDATE orders SET business_date = business_date_of(created_at, tenant_id, branch_id);
UPDATE payments SET business_date = business_date_of(created_at, tenant_id, branch_id);
UPDATE credit_sales SET business_date = business_date_of(created_at, tenant_id, branch_id);
UPDATE stock_movements SET business_date = business_date_of(created_at, tenant_id, branch_id);
UPDATE sale_returns r SET business_date = business_date_of(r.created_at, r.tenant_id, s.branch_id) FROM sales s WHERE s.id = r.sale_id;

ALTER TABLE sales ALTER COLUMN business_date SET NOT NULL;
ALTER TABLE orders ALTER COLUMN business_date SET NOT NULL;
ALTER TABLE payments ALTER COLUMN business_date SET NOT NULL;
ALTER TABLE credit_sales ALTER COLUMN business_date SET NOT NULL;
ALTER TABLE stock_movements ALTER COLUMN business_date SET NOT NULL;
ALTER TABLE sale_returns ALTER COLUMN business_date SET NOT NULL;

-- Filled by the database for every insert (and when a timestamp is corrected), whatever code path writes the row.
CREATE FUNCTION set_business_date() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.business_date := business_date_of(NEW.created_at, NEW.tenant_id, NEW.branch_id);
    RETURN NEW;
END $$;

CREATE FUNCTION set_return_business_date() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.business_date := business_date_of(NEW.created_at, NEW.tenant_id, (SELECT branch_id FROM sales WHERE id = NEW.sale_id));
    RETURN NEW;
END $$;

CREATE TRIGGER sales_business_date BEFORE INSERT OR UPDATE OF created_at ON sales FOR EACH ROW EXECUTE FUNCTION set_business_date();
CREATE TRIGGER orders_business_date BEFORE INSERT OR UPDATE OF created_at ON orders FOR EACH ROW EXECUTE FUNCTION set_business_date();
CREATE TRIGGER payments_business_date BEFORE INSERT OR UPDATE OF created_at ON payments FOR EACH ROW EXECUTE FUNCTION set_business_date();
CREATE TRIGGER credit_sales_business_date BEFORE INSERT OR UPDATE OF created_at ON credit_sales FOR EACH ROW EXECUTE FUNCTION set_business_date();
CREATE TRIGGER stock_movements_business_date BEFORE INSERT OR UPDATE OF created_at ON stock_movements FOR EACH ROW EXECUTE FUNCTION set_business_date();
CREATE TRIGGER sale_returns_business_date BEFORE INSERT OR UPDATE OF created_at ON sale_returns FOR EACH ROW EXECUTE FUNCTION set_return_business_date();

CREATE INDEX sales_business_date_idx ON sales (tenant_id, business_date);
CREATE INDEX orders_business_date_idx ON orders (tenant_id, business_date);
CREATE INDEX payments_business_date_idx ON payments (tenant_id, business_date);
CREATE INDEX stock_movements_business_date_idx ON stock_movements (tenant_id, business_date);

-- Sales outside trading hours are allowed by default; when a business switches to "block", the roles that already
-- supervise the counter (may approve discount overrides) keep the ability to sell after hours.
UPDATE roles SET permissions = array_append(permissions, 'sales.outside_hours')
 WHERE NOT ('*' = ANY(permissions)) AND NOT ('sales.outside_hours' = ANY(permissions))
   AND 'sales.discount_override' = ANY(permissions);
