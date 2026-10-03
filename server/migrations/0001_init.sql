-- S'Shop core schema.
-- Conventions:
--   * every business table carries tenant_id; all queries are tenant-scoped
--   * money is NUMERIC(14,2); loyalty points are BIGINT
--   * enumerations are TEXT + CHECK so new values are a one-line migration
--   * financial/operational records are never hard-deleted (status changes instead)

-- ───────────────────────────── Tenancy & access ─────────────────────────────

CREATE TABLE tenants (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name        text NOT NULL,
    slug        text NOT NULL UNIQUE,
    tagline     text NOT NULL DEFAULT '',
    phone       text NOT NULL DEFAULT '',
    email       text NOT NULL DEFAULT '',
    address     text NOT NULL DEFAULT '',
    currency    text NOT NULL DEFAULT 'KSh',
    timezone    text NOT NULL DEFAULT 'Africa/Nairobi',
    logo        bytea,
    logo_mime   text,
    settings    jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE roles (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name        text NOT NULL,
    description text NOT NULL DEFAULT '',
    permissions text[] NOT NULL DEFAULT '{}',
    is_system   boolean NOT NULL DEFAULT false,
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name)
);

CREATE TABLE branches (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name        text NOT NULL,
    code        text NOT NULL,
    location    text NOT NULL DEFAULT '',
    phone       text NOT NULL DEFAULT '',
    manager_id  uuid,
    is_active   boolean NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, code)
);

CREATE TABLE users (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id         uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name              text NOT NULL,
    email             text NOT NULL,
    phone             text NOT NULL DEFAULT '',
    pin_hash          text NOT NULL,
    role_id           uuid NOT NULL REFERENCES roles(id),
    is_active         boolean NOT NULL DEFAULT true,
    all_branches      boolean NOT NULL DEFAULT false,
    failed_attempts   int NOT NULL DEFAULT 0,
    locked_until      timestamptz,
    last_login_at     timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_email_uq ON users (lower(email));

ALTER TABLE branches ADD CONSTRAINT branches_manager_fk
    FOREIGN KEY (manager_id) REFERENCES users(id) ON DELETE SET NULL;

CREATE TABLE user_branches (
    user_id   uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    branch_id uuid NOT NULL REFERENCES branches(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, branch_id)
);

-- Sequential document numbers (ORD-2026-000001, RCP-2026-000001 …)
CREATE TABLE doc_counters (
    tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind      text NOT NULL,
    year      int  NOT NULL,
    value     bigint NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, kind, year)
);

-- ───────────────────────────── Catalogue ─────────────────────────────

CREATE TABLE categories (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name       text NOT NULL,
    is_active  boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX categories_name_uq ON categories (tenant_id, lower(name));

CREATE TABLE suppliers (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name       text NOT NULL,
    phone      text NOT NULL DEFAULT '',
    email      text NOT NULL DEFAULT '',
    notes      text NOT NULL DEFAULT '',
    is_active  boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX suppliers_name_uq ON suppliers (tenant_id, lower(name));

CREATE TABLE products (
    id                   uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id            uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    code                 text NOT NULL,
    name                 text NOT NULL,
    nickname             text NOT NULL DEFAULT '',
    description          text NOT NULL DEFAULT '',
    category_id          uuid REFERENCES categories(id) ON DELETE SET NULL,
    supplier_id          uuid REFERENCES suppliers(id) ON DELETE SET NULL,
    marked_price         numeric(14,2) NOT NULL DEFAULT 0 CHECK (marked_price >= 0),
    max_discount         numeric(14,2) CHECK (max_discount IS NULL OR max_discount >= 0),
    cost_price           numeric(14,2) CHECK (cost_price IS NULL OR cost_price >= 0),
    barcode              text,                      -- product-level (shared) barcode
    track_items          boolean NOT NULL DEFAULT false, -- every unit has its own barcode
    is_active            boolean NOT NULL DEFAULT true,
    available_for_orders boolean NOT NULL DEFAULT true,
    transfer_allowed     boolean NOT NULL DEFAULT true,
    loyalty_eligible     boolean NOT NULL DEFAULT true,
    loyalty_threshold    numeric(14,2) CHECK (loyalty_threshold IS NULL OR loyalty_threshold > 0),
    loyalty_points_per   int CHECK (loyalty_points_per IS NULL OR loyalty_points_per >= 0),
    low_stock_threshold  int CHECK (low_stock_threshold IS NULL OR low_stock_threshold >= 0),
    all_branches         boolean NOT NULL DEFAULT true,
    created_by           uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, code)
);
CREATE INDEX products_barcode_idx ON products (tenant_id, barcode) WHERE barcode IS NOT NULL;
CREATE INDEX products_name_idx ON products (tenant_id, lower(name));

CREATE TABLE product_branches (
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    branch_id  uuid NOT NULL REFERENCES branches(id) ON DELETE CASCADE,
    PRIMARY KEY (product_id, branch_id)
);

CREATE TABLE product_photos (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    data       bytea NOT NULL,
    mime       text NOT NULL,
    is_primary boolean NOT NULL DEFAULT false,
    sort_order int NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX product_photos_product_idx ON product_photos (product_id, sort_order);
CREATE UNIQUE INDEX product_photos_primary_uq ON product_photos (product_id) WHERE is_primary;

-- ───────────────────────────── Inventory ledger ─────────────────────────────

-- Projection of the ledger, maintained in the same DB transaction as every
-- movement. It exists so rows can be locked (SELECT … FOR UPDATE) to prevent
-- overselling; the ledger (stock_movements) remains the source of truth.
CREATE TABLE stock_levels (
    tenant_id  uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    branch_id  uuid NOT NULL REFERENCES branches(id) ON DELETE CASCADE,
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    on_hand    int NOT NULL DEFAULT 0,
    reserved   int NOT NULL DEFAULT 0 CHECK (reserved >= 0),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (branch_id, product_id)
);

CREATE TABLE stock_items (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    product_id  uuid NOT NULL REFERENCES products(id),
    branch_id   uuid NOT NULL REFERENCES branches(id),
    barcode     text NOT NULL,
    status      text NOT NULL DEFAULT 'in_stock'
                CHECK (status IN ('in_stock','reserved','in_transit','sold','written_off','returned_to_supplier')),
    cost_price  numeric(14,2),
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);
-- A barcode may identify only one *active* physical unit.
CREATE UNIQUE INDEX stock_items_active_barcode_uq ON stock_items (tenant_id, barcode)
    WHERE status IN ('in_stock','reserved','in_transit');
CREATE INDEX stock_items_lookup_idx ON stock_items (tenant_id, product_id, branch_id, status);

CREATE TABLE stock_movements (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    branch_id     uuid NOT NULL REFERENCES branches(id),
    product_id    uuid NOT NULL REFERENCES products(id),
    stock_item_id uuid REFERENCES stock_items(id),
    kind          text NOT NULL CHECK (kind IN (
                    'opening','received','sale','order_completion','transfer_out','transfer_in',
                    'customer_return','supplier_return','damage','loss','write_off','adjustment',
                    'count_variance','sale_reversal')),
    quantity      int NOT NULL CHECK (quantity <> 0),
    unit_cost     numeric(14,2),
    unit_price    numeric(14,2),
    ref_type      text,
    ref_id        uuid,
    supplier_id   uuid REFERENCES suppliers(id) ON DELETE SET NULL,
    notes         text NOT NULL DEFAULT '',
    user_id       uuid REFERENCES users(id) ON DELETE SET NULL,
    occurred_on   date NOT NULL DEFAULT CURRENT_DATE,
    created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX stock_movements_lookup_idx ON stock_movements (tenant_id, branch_id, product_id, created_at);
CREATE INDEX stock_movements_ref_idx ON stock_movements (ref_type, ref_id);
CREATE INDEX stock_movements_item_idx ON stock_movements (stock_item_id) WHERE stock_item_id IS NOT NULL;

-- ───────────────────────────── Customers & loyalty ─────────────────────────────

CREATE TABLE customers (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    mobile           text NOT NULL,
    first_name       text NOT NULL,
    other_names      text NOT NULL DEFAULT '',
    nickname         text NOT NULL DEFAULT '',
    email            text NOT NULL DEFAULT '',
    custom_fields    jsonb NOT NULL DEFAULT '{}'::jsonb,
    total_spend      numeric(14,2) NOT NULL DEFAULT 0,
    purchase_count   int NOT NULL DEFAULT 0,
    last_purchase_at timestamptz,
    own_points       bigint NOT NULL DEFAULT 0,  -- earned from own purchases
    referral_points  bigint NOT NULL DEFAULT 0,  -- earned from referred customers
    points_redeemed  bigint NOT NULL DEFAULT 0,
    points_expired   bigint NOT NULL DEFAULT 0,
    points_available bigint NOT NULL DEFAULT 0,
    tier             text NOT NULL DEFAULT '',
    is_active        boolean NOT NULL DEFAULT true,
    created_by       uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, mobile)
);
CREATE INDEX customers_name_idx ON customers (tenant_id, lower(first_name));

CREATE TABLE customer_fields (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    key           text NOT NULL,
    label         text NOT NULL,
    field_type    text NOT NULL CHECK (field_type IN ('text','number','date','dropdown','boolean','email')),
    options       text[] NOT NULL DEFAULT '{}',
    required      boolean NOT NULL DEFAULT false,
    is_active     boolean NOT NULL DEFAULT true,
    display_order int NOT NULL DEFAULT 0,
    UNIQUE (tenant_id, key)
);

CREATE TABLE referrals (
    id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    referrer_id         uuid NOT NULL REFERENCES customers(id),
    referred_id         uuid NOT NULL REFERENCES customers(id),
    bonus_points_earned bigint NOT NULL DEFAULT 0,
    is_active           boolean NOT NULL DEFAULT true,
    created_by          uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at          timestamptz NOT NULL DEFAULT now(),
    CHECK (referrer_id <> referred_id)
);
CREATE UNIQUE INDEX referrals_referred_uq ON referrals (tenant_id, referred_id) WHERE is_active;

CREATE TABLE award_periods (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name       text NOT NULL,
    start_date date NOT NULL,
    end_date   date,
    status     text NOT NULL DEFAULT 'open' CHECK (status IN ('open','closed')),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX award_periods_one_open ON award_periods (tenant_id) WHERE status = 'open';

CREATE TABLE award_winners (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    period_id     uuid NOT NULL REFERENCES award_periods(id) ON DELETE CASCADE,
    customer_id   uuid NOT NULL REFERENCES customers(id),
    customer_name text NOT NULL,
    tier          text NOT NULL,
    rank          int NOT NULL,
    total_spend   numeric(14,2) NOT NULL DEFAULT 0,
    points        bigint NOT NULL DEFAULT 0
);

-- ───────────────────────────── Sales & payments ─────────────────────────────

CREATE TABLE orders (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id         uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    branch_id         uuid NOT NULL REFERENCES branches(id),
    order_no          text NOT NULL,
    customer_id       uuid NOT NULL REFERENCES customers(id),
    status            text NOT NULL DEFAULT 'new' CHECK (status IN (
                        'new','confirmed','preparing','dispatched','on_delivery','delivered','completed',
                        'cancelled','rejected','returned')),
    source            text NOT NULL DEFAULT 'portal' CHECK (source IN ('portal','internal')),
    delivery_location text NOT NULL DEFAULT '',
    notes             text NOT NULL DEFAULT '',
    total             numeric(14,2) NOT NULL DEFAULT 0,
    reserved          boolean NOT NULL DEFAULT false,
    sale_id           uuid,
    track_token       uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    created_by        uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, order_no)
);
CREATE INDEX orders_status_idx ON orders (tenant_id, branch_id, status, created_at DESC);
CREATE INDEX orders_customer_idx ON orders (customer_id, created_at DESC);

CREATE TABLE order_items (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id   uuid NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    product_id uuid NOT NULL REFERENCES products(id),
    quantity   int NOT NULL CHECK (quantity > 0),
    unit_price numeric(14,2) NOT NULL,
    line_total numeric(14,2) NOT NULL
);

CREATE TABLE order_events (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id   uuid NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    status     text NOT NULL,
    user_id    uuid REFERENCES users(id) ON DELETE SET NULL,
    notes      text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sales (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    branch_id        uuid NOT NULL REFERENCES branches(id),
    receipt_no       text NOT NULL,
    customer_id      uuid REFERENCES customers(id),
    user_id          uuid REFERENCES users(id) ON DELETE SET NULL,
    order_id         uuid REFERENCES orders(id),
    status           text NOT NULL DEFAULT 'completed'
                     CHECK (status IN ('completed','partially_returned','returned','cancelled')),
    gross_total      numeric(14,2) NOT NULL DEFAULT 0,  -- Σ marked price × qty
    discount_total   numeric(14,2) NOT NULL DEFAULT 0,  -- Σ (marked − selling) × qty (negative = premium)
    total            numeric(14,2) NOT NULL DEFAULT 0,  -- Σ selling price × qty − points redemption
    redeemed_points  bigint NOT NULL DEFAULT 0,
    redeemed_value   numeric(14,2) NOT NULL DEFAULT 0,
    amount_paid      numeric(14,2) NOT NULL DEFAULT 0,
    payment_method   text NOT NULL,
    points_earned    bigint NOT NULL DEFAULT 0,
    approved_by      uuid REFERENCES users(id) ON DELETE SET NULL,
    notes            text NOT NULL DEFAULT '',
    is_legacy        boolean NOT NULL DEFAULT false,
    client_ref       uuid,  -- idempotency key from the POS so a retried submit never double-sells
    cancelled_at     timestamptz,
    cancelled_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    cancel_reason    text,
    created_at       timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, receipt_no)
);
CREATE INDEX sales_period_idx ON sales (tenant_id, branch_id, created_at DESC);
CREATE UNIQUE INDEX sales_client_ref_uq ON sales (tenant_id, client_ref) WHERE client_ref IS NOT NULL;
CREATE INDEX sales_customer_idx ON sales (customer_id, created_at DESC);
ALTER TABLE orders ADD CONSTRAINT orders_sale_fk FOREIGN KEY (sale_id) REFERENCES sales(id);

CREATE TABLE sale_items (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    sale_id       uuid NOT NULL REFERENCES sales(id),
    product_id    uuid NOT NULL REFERENCES products(id),
    stock_item_id uuid REFERENCES stock_items(id),
    barcode       text,
    quantity      int NOT NULL CHECK (quantity > 0),
    returned_qty  int NOT NULL DEFAULT 0 CHECK (returned_qty >= 0),
    marked_price  numeric(14,2) NOT NULL,
    unit_price    numeric(14,2) NOT NULL CHECK (unit_price >= 0),
    line_total    numeric(14,2) NOT NULL,
    unit_cost     numeric(14,2),
    points        bigint NOT NULL DEFAULT 0,
    CHECK (returned_qty <= quantity)
);
CREATE INDEX sale_items_sale_idx ON sale_items (sale_id);
CREATE INDEX sale_items_product_idx ON sale_items (tenant_id, product_id);

CREATE TABLE mpesa_requests (
    id                   uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id            uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    branch_id            uuid REFERENCES branches(id),
    merchant_request_id  text,
    checkout_request_id  text UNIQUE,
    phone                text NOT NULL,
    amount               numeric(14,2) NOT NULL,
    account_ref          text NOT NULL,
    status               text NOT NULL DEFAULT 'pending'
                         CHECK (status IN ('pending','success','failed','cancelled','timeout')),
    result_code          int,
    result_desc          text,
    mpesa_receipt        text,
    consumed_by          uuid,       -- sale or credit payment that used this confirmation
    raw_callback         jsonb,
    user_id              uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE credit_sales (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    branch_id       uuid NOT NULL REFERENCES branches(id),
    sale_id         uuid NOT NULL UNIQUE REFERENCES sales(id),
    customer_id     uuid NOT NULL REFERENCES customers(id),
    user_id         uuid REFERENCES users(id) ON DELETE SET NULL,
    original_amount numeric(14,2) NOT NULL,
    amount_paid     numeric(14,2) NOT NULL DEFAULT 0,
    adjustments     numeric(14,2) NOT NULL DEFAULT 0, -- reductions from returns
    due_date        date NOT NULL,
    status          text NOT NULL DEFAULT 'outstanding'
                    CHECK (status IN ('outstanding','partially_paid','paid','written_off','cancelled')),
    written_off_at  timestamptz,
    written_off_by  uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX credit_sales_status_idx ON credit_sales (tenant_id, status, due_date);

CREATE TABLE payments (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    branch_id        uuid NOT NULL REFERENCES branches(id),
    sale_id          uuid REFERENCES sales(id),
    credit_sale_id   uuid REFERENCES credit_sales(id),
    method           text NOT NULL,
    amount           numeric(14,2) NOT NULL,   -- negative for refunds
    reference        text NOT NULL DEFAULT '',
    phone            text NOT NULL DEFAULT '',
    mpesa_request_id uuid REFERENCES mpesa_requests(id),
    user_id          uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at       timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX payments_sale_idx ON payments (sale_id);
-- An M-Pesa confirmation code can settle only one payment.
CREATE UNIQUE INDEX payments_mpesa_ref_uq ON payments (tenant_id, upper(reference))
    WHERE method = 'mpesa' AND reference <> '' AND amount > 0;
CREATE INDEX payments_credit_idx ON payments (credit_sale_id);

CREATE TABLE sale_returns (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    sale_id        uuid NOT NULL REFERENCES sales(id),
    return_no      text NOT NULL,
    kind           text NOT NULL CHECK (kind IN ('return','cancellation')),
    reason         text NOT NULL,
    refund_amount  numeric(14,2) NOT NULL DEFAULT 0,
    refund_method  text NOT NULL DEFAULT '',
    restock        boolean NOT NULL DEFAULT true,
    points_reversed bigint NOT NULL DEFAULT 0,
    user_id        uuid REFERENCES users(id) ON DELETE SET NULL,
    approved_by    uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, return_no)
);

CREATE TABLE sale_return_items (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    return_id    uuid NOT NULL REFERENCES sale_returns(id) ON DELETE CASCADE,
    sale_item_id uuid NOT NULL REFERENCES sale_items(id),
    quantity     int NOT NULL CHECK (quantity > 0),
    amount       numeric(14,2) NOT NULL
);

CREATE TABLE loyalty_ledger (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    customer_id uuid NOT NULL REFERENCES customers(id),
    kind        text NOT NULL CHECK (kind IN ('earn','referral','redeem','expire','adjust','reversal')),
    points      bigint NOT NULL,
    sale_id     uuid REFERENCES sales(id),
    referral_id uuid REFERENCES referrals(id),
    expires_at  date,
    notes       text NOT NULL DEFAULT '',
    user_id     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX loyalty_ledger_customer_idx ON loyalty_ledger (customer_id, created_at DESC);

-- ───────────────────────────── Transfers & adjustments ─────────────────────────────

CREATE TABLE transfers (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    transfer_no    text NOT NULL,
    from_branch_id uuid NOT NULL REFERENCES branches(id),
    to_branch_id   uuid NOT NULL REFERENCES branches(id),
    status         text NOT NULL DEFAULT 'draft' CHECK (status IN (
                     'draft','pending_approval','approved','dispatched','received','cancelled','rejected')),
    transfer_date  date NOT NULL DEFAULT CURRENT_DATE,
    notes          text NOT NULL DEFAULT '',
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    approved_by    uuid REFERENCES users(id) ON DELETE SET NULL,
    dispatched_by  uuid REFERENCES users(id) ON DELETE SET NULL,
    received_by    uuid REFERENCES users(id) ON DELETE SET NULL,
    approved_at    timestamptz,
    dispatched_at  timestamptz,
    received_at    timestamptz,
    created_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, transfer_no),
    CHECK (from_branch_id <> to_branch_id)
);

CREATE TABLE transfer_items (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    transfer_id   uuid NOT NULL REFERENCES transfers(id) ON DELETE CASCADE,
    product_id    uuid NOT NULL REFERENCES products(id),
    stock_item_id uuid REFERENCES stock_items(id),
    quantity      int NOT NULL CHECK (quantity > 0)
);

CREATE TABLE stock_adjustments (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    branch_id     uuid NOT NULL REFERENCES branches(id),
    product_id    uuid NOT NULL REFERENCES products(id),
    stock_item_id uuid REFERENCES stock_items(id),
    kind          text NOT NULL CHECK (kind IN ('count','damage','loss','customer_return','supplier_return','manual','write_off')),
    previous_qty  int NOT NULL,
    delta         int NOT NULL,
    new_qty       int NOT NULL,
    reason        text NOT NULL,
    status        text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','applied','rejected')),
    created_by    uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_by    uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_at    timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now()
);

-- ───────────────────────────── Expenses ─────────────────────────────

CREATE TABLE expense_categories (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name       text NOT NULL,
    is_active  boolean NOT NULL DEFAULT true
);
CREATE UNIQUE INDEX expense_categories_name_uq ON expense_categories (tenant_id, lower(name));

CREATE TABLE expenses (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    branch_id       uuid NOT NULL REFERENCES branches(id),
    category_id     uuid NOT NULL REFERENCES expense_categories(id),
    amount          numeric(14,2) NOT NULL CHECK (amount > 0),
    expense_date    date NOT NULL,
    description     text NOT NULL DEFAULT '',
    payee           text NOT NULL DEFAULT '',
    payment_method  text NOT NULL DEFAULT 'cash',
    attachment      bytea,
    attachment_mime text,
    status          text NOT NULL DEFAULT 'approved' CHECK (status IN ('pending','approved','rejected','void')),
    user_id         uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX expenses_period_idx ON expenses (tenant_id, branch_id, expense_date);

-- ───────────────────────────── Workflow, audit, notifications ─────────────────────────────

CREATE TABLE workflows (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    action           text NOT NULL,
    enabled          boolean NOT NULL DEFAULT false,
    approver_type    text NOT NULL DEFAULT 'admin' CHECK (approver_type IN ('user','role','branch_manager','admin')),
    approver_role_id uuid REFERENCES roles(id) ON DELETE SET NULL,
    approver_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    min_amount       numeric(14,2),
    UNIQUE (tenant_id, action)
);

CREATE TABLE approvals (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    action       text NOT NULL,
    entity_type  text NOT NULL,
    entity_id    uuid NOT NULL,
    branch_id    uuid REFERENCES branches(id),
    summary      text NOT NULL,
    amount       numeric(14,2),
    payload      jsonb NOT NULL DEFAULT '{}'::jsonb, -- the deferred request, executed on approval
    status       text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','approved','rejected','cancelled')),
    requested_by uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_by   uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_at   timestamptz,
    comments     text NOT NULL DEFAULT '',
    created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX approvals_pending_idx ON approvals (tenant_id, status, created_at DESC);
CREATE INDEX approvals_entity_idx ON approvals (entity_type, entity_id);

CREATE TABLE audit_log (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id     uuid REFERENCES users(id) ON DELETE SET NULL,
    module      text NOT NULL,
    action      text NOT NULL,
    entity_type text NOT NULL,
    entity_id   uuid,
    branch_id   uuid REFERENCES branches(id) ON DELETE SET NULL,
    before      jsonb,
    after       jsonb,
    approval_id uuid REFERENCES approvals(id) ON DELETE SET NULL,
    comments    text NOT NULL DEFAULT '',
    ip          text NOT NULL DEFAULT '',
    user_agent  text NOT NULL DEFAULT '',
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_log_idx ON audit_log (tenant_id, created_at DESC);
CREATE INDEX audit_log_entity_idx ON audit_log (entity_type, entity_id);

CREATE TABLE notifications (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind       text NOT NULL,
    title      text NOT NULL,
    body       text NOT NULL DEFAULT '',
    link       text NOT NULL DEFAULT '',
    dedupe_key text,
    read_at    timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX notifications_user_idx ON notifications (user_id, created_at DESC);
CREATE UNIQUE INDEX notifications_dedupe_uq ON notifications (user_id, dedupe_key) WHERE dedupe_key IS NOT NULL;

CREATE TABLE whatsapp_messages (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     uuid REFERENCES tenants(id) ON DELETE CASCADE,
    direction     text NOT NULL CHECK (direction IN ('in','out')),
    phone         text NOT NULL,
    body          text NOT NULL DEFAULT '',
    wa_message_id text,
    status        text NOT NULL DEFAULT '',
    error         text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX whatsapp_messages_wa_idx ON whatsapp_messages (wa_message_id);

-- Ordering-portal one-time codes (mobile verification)
CREATE TABLE portal_otps (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    mobile     text NOT NULL,
    code_hash  text NOT NULL,
    attempts   int NOT NULL DEFAULT 0,
    used       boolean NOT NULL DEFAULT false,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX portal_otps_idx ON portal_otps (tenant_id, mobile, created_at DESC);
