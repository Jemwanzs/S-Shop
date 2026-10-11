-- Roadmap 91: each business's own M-Pesa (Daraja) accounts, and sales that wait for an STK Push confirmation.

-- A business's Till (Buy Goods) or Paybill, with its Daraja app. Secrets are encrypted by the server (AES-256-GCM);
-- the callback token is stored encrypted (to build the callback URL) and as a SHA-256 digest (to look it up).
CREATE TABLE mpesa_accounts (
    id                    uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id             uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name                  text NOT NULL,
    kind                  text NOT NULL CHECK (kind IN ('till', 'paybill')),
    -- Paybill number, or the Store / Head Office number of a Till (the Lipa na M-Pesa Online shortcode).
    shortcode             text NOT NULL,
    -- Buy Goods: the Till number customers pay (PartyB). Empty for a Paybill.
    till_number           text NOT NULL DEFAULT '',
    -- Paybill: the account reference customers see (default: the receipt or business name).
    account_reference     text NOT NULL DEFAULT '',
    environment           text NOT NULL DEFAULT 'sandbox' CHECK (environment IN ('sandbox', 'production')),
    consumer_key          text NOT NULL DEFAULT '',
    consumer_secret_enc   text NOT NULL DEFAULT '',
    passkey_enc           text NOT NULL DEFAULT '',
    callback_token_enc    text NOT NULL,
    callback_token_digest text NOT NULL UNIQUE,
    -- Payments may be taken (manual codes and, when STK is on, prompts).
    is_active             boolean NOT NULL DEFAULT true,
    -- STK Push stays off until a test prompt has been accepted by Safaricom with these exact credentials.
    stk_enabled           boolean NOT NULL DEFAULT false,
    verified_at           timestamptz,
    last_tested_at        timestamptz,
    last_error            text,
    last_error_at         timestamptz,
    all_branches          boolean NOT NULL DEFAULT true,
    branch_ids            uuid[] NOT NULL DEFAULT '{}',
    created_by            uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now(),
    CHECK (stk_enabled = false OR verified_at IS NOT NULL)
);
CREATE INDEX mpesa_accounts_tenant_idx ON mpesa_accounts (tenant_id);

ALTER TABLE mpesa_requests
    ADD COLUMN account_id   uuid REFERENCES mpesa_accounts(id) ON DELETE SET NULL,
    -- What the request is for: a sale waiting for it, a test prompt, or a payment taken first (deposits, credit, orders).
    ADD COLUMN purpose      text NOT NULL DEFAULT 'payment' CHECK (purpose IN ('payment', 'sale', 'test')),
    -- The amount Safaricom reported (kept apart from the requested amount, which never changes).
    ADD COLUMN paid_amount  numeric(14,2),
    -- How the outcome was learned: callback or status query.
    ADD COLUMN confirmed_by text;

-- A sale waiting for its STK Push: validated in full, stock held, completed exactly once on verified payment.
CREATE TABLE pending_sales (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    branch_id        uuid NOT NULL REFERENCES branches(id),
    user_id          uuid REFERENCES users(id) ON DELETE SET NULL,
    -- Set once the prompt is accepted (stock is held first, without holding locks during the call to Safaricom).
    mpesa_request_id uuid UNIQUE REFERENCES mpesa_requests(id),
    phone            text NOT NULL,
    -- The supervisor who approved a discount, verified when the sale was submitted (never their PIN).
    approved_by      uuid REFERENCES users(id) ON DELETE SET NULL,
    client_ref       uuid,
    amount           numeric(14,2) NOT NULL,
    -- The sale exactly as submitted, and the cashier's context it was validated with.
    body             jsonb NOT NULL,
    context          jsonb NOT NULL,
    -- Stock held for it: [{product_id, quantity}]; released when it completes, fails, is cancelled or times out.
    holds            jsonb NOT NULL DEFAULT '[]',
    held             boolean NOT NULL DEFAULT true,
    status           text NOT NULL DEFAULT 'awaiting'
                     CHECK (status IN ('awaiting', 'completed', 'failed', 'cancelled', 'timeout', 'attention')),
    sale_id          uuid REFERENCES sales(id),
    error            text,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX pending_sales_open_idx ON pending_sales (tenant_id, status) WHERE status IN ('awaiting', 'attention');
-- One open STK sale per cart: a second tap while one is waiting cannot send another prompt.
CREATE UNIQUE INDEX pending_sales_cart_uq ON pending_sales (tenant_id, client_ref) WHERE status = 'awaiting' AND client_ref IS NOT NULL;
