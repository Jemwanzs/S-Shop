-- Roadmap 1a: configurable product fields (Settings → Product Configuration → Product Fields).
-- Same shape as customer_fields; values live on products.custom_fields.

ALTER TABLE products ADD COLUMN custom_fields jsonb NOT NULL DEFAULT '{}'::jsonb;

CREATE TABLE product_fields (
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
