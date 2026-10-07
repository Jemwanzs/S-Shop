-- Roadmap 41–46: tenant packages (full platform or selected modules), tenant-specific pricing with discount and
-- tax, free / trial / grace access, billing suspension and the platform owner's protected business.

-- 45. Ownership. The platform owner's own business is 'platform': never deactivated, never suspended, never billed.
-- Marked at start-up for every business with an active platform administrator (PLATFORM_ADMIN_EMAILS).
ALTER TABLE tenants
    ADD COLUMN ownership         text NOT NULL DEFAULT 'customer' CHECK (ownership IN ('customer', 'platform')),
    -- 43. Billing suspension (overdue past grace, when the plan allows it): sign-in still works so the business
    -- can pay, everything else is refused until the overdue invoices are paid.
    ADD COLUMN billing_suspended boolean NOT NULL DEFAULT false;

-- 41–43. Package, pricing, discount, tax and access on the existing one-plan-per-business table.
ALTER TABLE billing_plans
    ADD COLUMN package        text NOT NULL DEFAULT 'full' CHECK (package IN ('full', 'modules')),
    ADD COLUMN modules        text[] NOT NULL DEFAULT '{}',
    -- Per-module prices for this business ({"sales": 1500, …}); the recurring base is their sum for a module package.
    ADD COLUMN module_prices  jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN discount_type  text NOT NULL DEFAULT 'none' CHECK (discount_type IN ('none', 'percent', 'fixed')),
    ADD COLUMN discount_value numeric(14,2) NOT NULL DEFAULT 0 CHECK (discount_value >= 0),
    ADD COLUMN tax_enabled    boolean NOT NULL DEFAULT false,
    ADD COLUMN tax_rate       numeric(5,2) NOT NULL DEFAULT 0 CHECK (tax_rate BETWEEN 0 AND 100),
    -- billed (normal), free (billing switched off, business stays active) or trial.
    ADD COLUMN access_mode    text NOT NULL DEFAULT 'billed' CHECK (access_mode IN ('billed', 'free', 'trial')),
    ADD COLUMN trial_start    date,
    ADD COLUMN trial_end      date,
    -- Modules during the trial (empty = the package's modules).
    ADD COLUMN trial_modules  text[] NOT NULL DEFAULT '{}',
    -- Explicit grace extension: overdue only after this date (on top of grace_days).
    ADD COLUMN grace_until    date,
    -- Suspend the business automatically when an invoice is overdue past its grace period.
    ADD COLUMN auto_suspend   boolean NOT NULL DEFAULT false,
    -- One-off fee paid outside S'Shop before billing was recorded here.
    ADD COLUMN one_off_paid_on date,
    ADD CONSTRAINT billing_plans_modules_ck CHECK (package = 'full' OR cardinality(modules) > 0),
    ADD CONSTRAINT billing_plans_trial_ck CHECK (access_mode <> 'trial' OR (trial_start IS NOT NULL AND trial_end IS NOT NULL AND trial_end >= trial_start)),
    ADD CONSTRAINT billing_plans_discount_ck CHECK (discount_type <> 'percent' OR discount_value <= 100);

-- Free access needs no price: replace 0014's unnamed table checks (billing_plans_check, _check1, _check2) with
-- named ones that allow it.
DO $$
DECLARE c record;
BEGIN
    FOR c IN SELECT conname FROM pg_constraint WHERE conrelid = 'billing_plans'::regclass AND contype = 'c' AND conname ~ '^billing_plans_check[0-9]*$'
    LOOP
        EXECUTE format('ALTER TABLE billing_plans DROP CONSTRAINT %I', c.conname);
    END LOOP;
END $$;
ALTER TABLE billing_plans
    ADD CONSTRAINT billing_plans_one_off_ck CHECK (access_mode = 'free' OR model = 'subscription' OR one_off_amount > 0),
    ADD CONSTRAINT billing_plans_recurring_ck CHECK (NOT recurring OR (amount > 0 AND start_date IS NOT NULL AND next_due_date IS NOT NULL)),
    ADD CONSTRAINT billing_plans_model_ck CHECK (access_mode = 'free' OR model = 'one_off' OR recurring);

-- Invoice / quotation breakdown: subtotal → discount → tax → amount (the amount payable).
ALTER TABLE billing_documents
    ADD COLUMN subtotal numeric(14,2),
    ADD COLUMN discount numeric(14,2) NOT NULL DEFAULT 0 CHECK (discount >= 0),
    ADD COLUMN tax_rate numeric(5,2) NOT NULL DEFAULT 0,
    ADD COLUMN tax      numeric(14,2) NOT NULL DEFAULT 0 CHECK (tax >= 0);
UPDATE billing_documents SET subtotal = amount;
ALTER TABLE billing_documents ALTER COLUMN subtotal SET NOT NULL;

-- 45. Database-level protection of the platform owner's business, behind the checks in the API.
CREATE FUNCTION protect_platform_tenant() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_TABLE_NAME = 'tenants' THEN
        IF NEW.ownership = 'platform' AND (NEW.status <> 'active' OR NEW.billing_suspended) THEN
            RAISE EXCEPTION 'The platform owner''s business cannot be deactivated or suspended' USING ERRCODE = 'check_violation';
        END IF;
        IF OLD.ownership = 'platform' AND NEW.ownership <> 'platform' THEN
            RAISE EXCEPTION 'Platform ownership cannot be removed' USING ERRCODE = 'check_violation';
        END IF;
    ELSIF EXISTS (SELECT 1 FROM tenants WHERE id = NEW.tenant_id AND ownership = 'platform') THEN
        RAISE EXCEPTION 'Platform billing does not apply to the platform owner''s business' USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END $$;

CREATE TRIGGER tenants_protect_platform BEFORE UPDATE ON tenants FOR EACH ROW EXECUTE FUNCTION protect_platform_tenant();
CREATE TRIGGER billing_plans_protect_platform BEFORE INSERT OR UPDATE ON billing_plans FOR EACH ROW EXECUTE FUNCTION protect_platform_tenant();
CREATE TRIGGER billing_documents_protect_platform BEFORE INSERT ON billing_documents FOR EACH ROW EXECUTE FUNCTION protect_platform_tenant();
CREATE TRIGGER billing_payments_protect_platform BEFORE INSERT ON billing_payments FOR EACH ROW EXECUTE FUNCTION protect_platform_tenant();
