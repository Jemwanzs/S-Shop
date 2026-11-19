-- Roadmap 176: one product, its own selling price per branch.
-- A product keeps one record, one code and its barcodes; `products.marked_price` is its default price. A branch may
-- have its own price (an override). Every place that prices a sale, an order or stock at a branch uses
-- effective_price(product, branch, default): the branch's own price, else the default.
CREATE TABLE product_branch_prices (
    tenant_id  uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    branch_id  uuid NOT NULL REFERENCES branches(id) ON DELETE CASCADE,
    price      numeric(14,2) NOT NULL CHECK (price >= 0),
    updated_by uuid REFERENCES users(id) ON DELETE SET NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (product_id, branch_id)
);
CREATE INDEX product_branch_prices_branch_idx ON product_branch_prices (tenant_id, branch_id);

-- The selling price of a product at a branch (NULL branch = the default). Inlined by the planner.
CREATE FUNCTION effective_price(p_product uuid, p_branch uuid, p_default numeric) RETURNS numeric
    LANGUAGE sql STABLE AS $$
    SELECT COALESCE((SELECT bp.price FROM product_branch_prices bp WHERE bp.product_id = p_product AND bp.branch_id = p_branch), p_default)
$$;

-- What a transfer proposes for the destination branch's price, per product. Stock approval and price authorisation are
-- separate: a price change applies only when someone allowed to set prices there authorised it, and only once the stock
-- has been received.
--   unchanged  — keep the destination price (the default)
--   requested  — a change waits for someone with *Set selling prices* at the destination
--   authorized — authorised; applied when the transfer is received
--   applied    — the destination price was changed
--   declined   — the change was refused (the destination keeps its price)
--   not_applied — the transfer was cancelled, so nothing changed
CREATE TABLE transfer_prices (
    transfer_id  uuid NOT NULL REFERENCES transfers(id) ON DELETE CASCADE,
    product_id   uuid NOT NULL REFERENCES products(id),
    source_price numeric(14,2),
    dest_price   numeric(14,2),
    action       text NOT NULL CHECK (action IN ('keep','source','custom')),
    new_price    numeric(14,2) CHECK (new_price IS NULL OR new_price >= 0),
    status       text NOT NULL CHECK (status IN ('unchanged','requested','authorized','applied','declined','not_applied')),
    decided_by   uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_at   timestamptz,
    note         text NOT NULL DEFAULT '',
    PRIMARY KEY (transfer_id, product_id),
    CHECK ((action = 'keep') = (new_price IS NULL))
);

-- *Set selling prices* (products.price): everyone who could change a price before keeps that ability — roles that edit
-- products or receive stock (receiving can update the price).
UPDATE roles SET permissions = array_append(permissions, 'products.price')
 WHERE NOT ('*' = ANY(permissions)) AND NOT ('products.price' = ANY(permissions))
   AND ('products.edit' = ANY(permissions) OR 'stock.add' = ANY(permissions));
