-- Roadmap 179: returned-item credit — money a customer actually paid for goods that came back, held to settle a credit
-- sale (or paid out later) instead of being refunded at once. Only real payments create it: a return on a credit sale
-- first reduces what is owed (no money changes hands for the unpaid part), exactly as before.
CREATE TABLE return_credits (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    branch_id      uuid NOT NULL REFERENCES branches(id),
    customer_id    uuid REFERENCES customers(id) ON DELETE SET NULL,
    source_sale_id uuid NOT NULL REFERENCES sales(id),
    return_id      uuid NOT NULL UNIQUE REFERENCES sale_returns(id),
    amount         numeric(14,2) NOT NULL CHECK (amount > 0),
    -- Applied to credit sales, carried into a replacement sale, or paid out; never more than the amount.
    used           numeric(14,2) NOT NULL DEFAULT 0 CHECK (used >= 0 AND used <= amount),
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX return_credits_open_idx ON return_credits (tenant_id, created_at DESC) WHERE used < amount;

-- A repayment made with a return credit names it (method 'return_credit').
ALTER TABLE payments ADD COLUMN return_credit_id uuid REFERENCES return_credits(id);
CREATE INDEX payments_return_credit_idx ON payments (return_credit_id) WHERE return_credit_id IS NOT NULL;

-- A replacement sale made by swapping a credit-sold item points to the sale it replaces.
ALTER TABLE sales ADD COLUMN replaces_sale_id uuid REFERENCES sales(id);

-- Recalls that left the customer overpaid kept the difference as "customer credit" with no way to use it: it becomes a
-- usable return credit (nothing used yet).
INSERT INTO return_credits (tenant_id, branch_id, customer_id, source_sale_id, return_id, amount, created_by, created_at)
SELECT r.tenant_id, s.branch_id, s.customer_id, r.sale_id, r.id, r.customer_credit, r.user_id, r.created_at
  FROM sale_returns r JOIN sales s ON s.id = r.sale_id
 WHERE r.customer_credit > 0;
