-- Roadmap 34–40: platform owner tenant directory, activation status, billing (plans, quotations, invoices,
-- payments, receipts) and Paystack. Platform-owner data lives in its own tables; a business only ever reads
-- its own rows (tenant_id), never another business's.

-- 36. Business status. Deactivation keeps every record; it blocks sign-in, ends sessions issued before it,
-- disables the ordering link and therefore blocks new transactions.
ALTER TABLE tenants
    ADD COLUMN status               text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'deactivated')),
    ADD COLUMN status_reason        text NOT NULL DEFAULT '',
    ADD COLUMN status_changed_at    timestamptz,
    ADD COLUMN activated_at         timestamptz DEFAULT now(),
    ADD COLUMN sessions_valid_after timestamptz;
UPDATE tenants SET activated_at = created_at;

-- Platform-wide settings owned by the platform owner (vendor bank details shown to businesses).
CREATE TABLE platform_settings (
    key        text PRIMARY KEY,
    value      jsonb NOT NULL,
    updated_by uuid REFERENCES users(id) ON DELETE SET NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);
INSERT INTO platform_settings (key, value) VALUES
    ('vendor', '{"bank_name": "I&M Bank", "account_name": "", "account_number": "450", "branch": "", "instructions": ""}');

-- 37. One billing plan per business.
--   subscription: a recurring fee (amount / frequency) from start_date.
--   one_off:      a one-off fee, optionally followed by a recurring maintenance fee (same recurring columns).
CREATE TABLE billing_plans (
    tenant_id      uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    model          text NOT NULL CHECK (model IN ('subscription', 'one_off')),
    currency       text NOT NULL DEFAULT 'KES',
    one_off_amount numeric(14,2) NOT NULL DEFAULT 0 CHECK (one_off_amount >= 0),
    recurring      boolean NOT NULL DEFAULT true,
    amount         numeric(14,2) NOT NULL DEFAULT 0 CHECK (amount >= 0),
    frequency      text NOT NULL DEFAULT 'monthly' CHECK (frequency IN ('monthly', 'quarterly', 'semi_annual', 'annual', 'custom')),
    custom_months  int NOT NULL DEFAULT 1 CHECK (custom_months BETWEEN 1 AND 60),
    start_date     date,
    next_due_date  date,
    grace_days     int NOT NULL DEFAULT 7 CHECK (grace_days BETWEEN 0 AND 90),
    auto_renew     boolean NOT NULL DEFAULT true,
    notes          text NOT NULL DEFAULT '',
    updated_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CHECK (model = 'subscription' OR one_off_amount > 0),
    CHECK (NOT recurring OR (amount > 0 AND start_date IS NOT NULL AND next_due_date IS NOT NULL)),
    CHECK (model = 'one_off' OR recurring)
);

CREATE SEQUENCE billing_quotation_no;
CREATE SEQUENCE billing_invoice_no;
CREATE SEQUENCE billing_receipt_no;

-- Quotations and invoices. An invoice may come from an accepted quotation.
--   quotation: open → accepted | declined | void
--   invoice:   open → paid | void          (overdue is derived from due_date + grace)
CREATE TABLE billing_documents (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind         text NOT NULL CHECK (kind IN ('quotation', 'invoice')),
    number       text NOT NULL UNIQUE,
    category     text NOT NULL CHECK (category IN ('subscription', 'one_off', 'maintenance', 'other')),
    description  text NOT NULL,
    amount       numeric(14,2) NOT NULL CHECK (amount > 0),
    currency     text NOT NULL DEFAULT 'KES',
    issue_date   date NOT NULL,
    due_date     date NOT NULL,
    period_start date,
    period_end   date,
    status       text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'accepted', 'declined', 'paid', 'void')),
    quotation_id uuid REFERENCES billing_documents(id) ON DELETE SET NULL,
    paid_at      timestamptz,
    void_reason  text NOT NULL DEFAULT '',
    created_by   uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CHECK (kind = 'quotation' OR status IN ('open', 'paid', 'void')),
    CHECK (kind = 'invoice' OR status IN ('open', 'accepted', 'declined', 'void')),
    CHECK ((period_start IS NULL) = (period_end IS NULL) AND (period_end IS NULL OR period_end >= period_start)),
    CHECK (due_date >= issue_date)
);
CREATE INDEX billing_documents_tenant_idx ON billing_documents (tenant_id, kind, created_at DESC);
-- One live invoice per billing period, so automatic renewal can never bill a period twice.
CREATE UNIQUE INDEX billing_documents_period_uq ON billing_documents (tenant_id, category, period_start)
    WHERE kind = 'invoice' AND status <> 'void' AND period_start IS NOT NULL;
-- An accepted quotation becomes exactly one invoice.
CREATE UNIQUE INDEX billing_documents_quotation_uq ON billing_documents (quotation_id) WHERE kind = 'invoice' AND status <> 'void';

-- Payments against invoices. Paystack payments start 'pending' and only become 'success' after the server has
-- verified them with Paystack (verify endpoint, signed webhook or reconciliation) — never on the browser's word.
-- Payments made outside S'Shop (bank transfer …) are recorded by the platform owner.
CREATE TABLE billing_payments (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    invoice_id  uuid NOT NULL REFERENCES billing_documents(id),
    amount      numeric(14,2) NOT NULL CHECK (amount > 0),
    currency    text NOT NULL DEFAULT 'KES',
    method      text NOT NULL CHECK (method IN ('paystack', 'bank', 'mpesa', 'cash', 'other')),
    reference   text NOT NULL UNIQUE,
    status      text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'success', 'failed', 'abandoned')),
    channel     text NOT NULL DEFAULT '',
    gateway     jsonb,
    receipt_no  text UNIQUE,
    paid_at     timestamptz,
    verified_at timestamptz,
    recorded_by uuid REFERENCES users(id) ON DELETE SET NULL,
    note        text NOT NULL DEFAULT '',
    created_at  timestamptz NOT NULL DEFAULT now(),
    CHECK (status <> 'success' OR (receipt_no IS NOT NULL AND paid_at IS NOT NULL))
);
CREATE INDEX billing_payments_tenant_idx ON billing_payments (tenant_id, created_at DESC);
CREATE INDEX billing_payments_pending_idx ON billing_payments (created_at) WHERE status = 'pending';
-- An invoice is marked paid once; a second successful payment (paid twice at Paystack) is kept with a note so it
-- can be refunded or credited — money that arrived is never discarded.

-- 35. Activity monitoring across businesses reads the audit trail by action and date.
CREATE INDEX audit_log_action_idx ON audit_log (module, action, created_at DESC);
