-- Roadmap 145: each business's workspace lives at https://s-shop.store/{slug}/… (the same slug as its ordering link
-- /order/{slug}, sign-in page /login/{slug} and website /s/{slug}). A slug is permanent by default; when an
-- administrator changes it, the old one is kept here so old links (workspace bookmarks, ordering links, websites)
-- keep reaching the business and the old slug is never given to another business.
CREATE TABLE tenant_slug_history (
    slug       text PRIMARY KEY,
    tenant_id  uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    changed_by uuid REFERENCES users(id) ON DELETE SET NULL,
    changed_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX tenant_slug_history_tenant_idx ON tenant_slug_history (tenant_id);

-- The business's current slug for a current or former slug (NULL when nobody has it).
CREATE OR REPLACE FUNCTION current_slug(s text) RETURNS text LANGUAGE sql STABLE AS $$
    SELECT COALESCE(
        (SELECT slug FROM tenants WHERE slug = s),
        (SELECT t.slug FROM tenant_slug_history h JOIN tenants t ON t.id = h.tenant_id WHERE h.slug = s)
    )
$$;
