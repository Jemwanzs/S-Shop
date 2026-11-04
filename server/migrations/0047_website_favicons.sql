-- Roadmap 161: each website's own favicon. A set of PNGs made in the browser from what the business uploaded, checked here.
-- A set never changes: replacing a favicon makes a new one, so its address can be cached for good and an earlier published
-- version of the website keeps its own icon. Which set is live is decided by the website document (`brand.favicon`).
CREATE TABLE website_favicons (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    png16       bytea NOT NULL,
    png32       bytea NOT NULL,
    png48       bytea NOT NULL,
    png180      bytea NOT NULL,
    png192      bytea NOT NULL,
    png512      bytea NOT NULL,
    source_name text NOT NULL DEFAULT '',
    created_by  uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX website_favicons_tenant_idx ON website_favicons (tenant_id, created_at DESC);
