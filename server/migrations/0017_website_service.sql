-- Roadmap 51: the Website Add-On as a billable service, requested by a business and activated by the platform owner.

-- Billing gets a service dimension: the same engine (plans, quotations, invoices, payments, receipts, renewal, grace,
-- suspension) bills the platform and the website separately for the same business.
ALTER TABLE billing_plans ADD COLUMN service text NOT NULL DEFAULT 'platform' CHECK (service IN ('platform', 'website'));
ALTER TABLE billing_plans DROP CONSTRAINT billing_plans_pkey;
ALTER TABLE billing_plans ADD PRIMARY KEY (tenant_id, service);

ALTER TABLE billing_documents ADD COLUMN service text NOT NULL DEFAULT 'platform' CHECK (service IN ('platform', 'website'));
DROP INDEX billing_documents_period_uq;
CREATE UNIQUE INDEX billing_documents_period_uq ON billing_documents (tenant_id, service, category, period_start)
    WHERE kind = 'invoice' AND status <> 'void' AND period_start IS NOT NULL;

-- One website per business. Draft and published configuration are documents (typed and validated in the server);
-- publishing copies the draft and keeps a numbered history for rollback.
CREATE TABLE websites (
    tenant_id         uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    -- requested | declined | active | disabled
    status            text NOT NULL CHECK (status IN ('requested', 'declined', 'active', 'disabled')),
    status_reason     text NOT NULL DEFAULT '',
    requested_by      uuid REFERENCES users(id) ON DELETE SET NULL,
    requested_at      timestamptz,
    request_message   text NOT NULL DEFAULT '',
    decided_by        uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_at        timestamptz,
    activated_at      timestamptz,
    -- Overdue website invoices (with automatic suspension) make the public website temporarily unavailable.
    billing_suspended boolean NOT NULL DEFAULT false,
    draft             jsonb NOT NULL DEFAULT '{}'::jsonb,
    draft_updated_at  timestamptz,
    draft_updated_by  uuid REFERENCES users(id) ON DELETE SET NULL,
    published         jsonb,
    version           int NOT NULL DEFAULT 0,
    published_at      timestamptz,
    published_by      uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at        timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE website_versions (
    tenant_id    uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    version      int NOT NULL,
    config       jsonb NOT NULL,
    published_at timestamptz NOT NULL DEFAULT now(),
    published_by uuid REFERENCES users(id) ON DELETE SET NULL,
    note         text NOT NULL DEFAULT '',
    PRIMARY KEY (tenant_id, version)
);

-- Website permissions can be given to individual existing users on top of their role (website.* only), so a person can
-- manage the website without operational access to sales, stock or finance.
ALTER TABLE users ADD COLUMN extra_permissions text[] NOT NULL DEFAULT '{}'
    CHECK (extra_permissions <@ ARRAY['website.view', 'website.content', 'website.products', 'website.photos', 'website.categories',
        'website.media', 'website.services', 'website.testimonials', 'website.design', 'website.navigation', 'website.seo',
        'website.domain', 'website.preview', 'website.publish', 'website.analytics']::text[]);

-- Orders placed on the website flow into the same orders engine, marked with their source.
ALTER TABLE orders DROP CONSTRAINT orders_source_check;
ALTER TABLE orders ADD CONSTRAINT orders_source_check CHECK (source IN ('portal', 'internal', 'website'));

-- The platform owner's protection applies to website billing as well (existing triggers cover both services).

-- Roadmap 55: tenant-isolated media library. Images are optimised in the browser before upload (a large version for
-- display and a small one for grids and thumbnails); the server checks the real format and records quality warnings.
CREATE TABLE website_media (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    kind       text NOT NULL CHECK (kind IN ('logo', 'banner', 'product', 'service', 'testimonial', 'about', 'promotion', 'other')),
    name       text NOT NULL DEFAULT '',
    mime       text NOT NULL,
    width      int NOT NULL,
    height     int NOT NULL,
    bytes      int NOT NULL,
    data       bytea NOT NULL,
    thumb      bytea,
    thumb_mime text,
    -- good | warning (refused images are never stored)
    quality    text NOT NULL DEFAULT 'good' CHECK (quality IN ('good', 'warning')),
    warnings   text[] NOT NULL DEFAULT '{}',
    archived   boolean NOT NULL DEFAULT false,
    -- One id per pending upload in the browser: a retried upload is stored once.
    upload_ref uuid,
    created_by uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX website_media_tenant_idx ON website_media (tenant_id, archived, created_at DESC);
CREATE UNIQUE INDEX website_media_upload_ref_uq ON website_media (tenant_id, upload_ref) WHERE upload_ref IS NOT NULL;

-- Roadmap 56: one custom domain per business. Only a verified domain maps to a business, and a domain belongs to one
-- business only.
CREATE TABLE website_domains (
    tenant_id   uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    domain      text NOT NULL,
    -- unconfigured | points_elsewhere | dns_required | verifying | connected | ssl_pending | active | misconfigured
    status      text NOT NULL DEFAULT 'dns_required',
    token       text NOT NULL,
    last_check  jsonb,
    checked_at  timestamptz,
    verified_at timestamptz,
    active_at   timestamptz,
    created_by  uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CHECK (domain = lower(domain) AND domain ~ '^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,}$')
);
CREATE UNIQUE INDEX website_domains_domain_uq ON website_domains (domain);

-- Roadmap 57: website analytics (no personal data: an anonymous per-browser id, only after consent when required).
CREATE TABLE website_events (
    id         bigserial PRIMARY KEY,
    tenant_id  uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    at         timestamptz NOT NULL DEFAULT now(),
    kind       text NOT NULL CHECK (kind IN ('visit', 'product_view', 'add_to_cart', 'order_start', 'order_complete')),
    visitor    text NOT NULL DEFAULT '',
    product_id uuid,
    order_id   uuid
);
CREATE INDEX website_events_tenant_idx ON website_events (tenant_id, at DESC);
