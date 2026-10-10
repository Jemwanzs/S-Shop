-- Roadmap 84–86: holiday & promotional campaigns on business websites. A campaign has its own lifecycle, separate from
-- the website draft / publish: draft → scheduled → live → expired (computed from `published` and the start / end times,
-- so publishing and expiry happen on time without a background job) → archived. Off by default for every business.
ALTER TABLE websites ADD COLUMN campaigns_enabled boolean NOT NULL DEFAULT false;

CREATE TABLE website_campaigns (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name         text NOT NULL,
    occasion     text NOT NULL,
    starts_at    timestamptz NOT NULL,
    ends_at      timestamptz NOT NULL,
    -- Published by someone with website.publish; shown only between starts_at and ends_at.
    published    boolean NOT NULL DEFAULT false,
    published_at timestamptz,
    published_by uuid REFERENCES users(id) ON DELETE SET NULL,
    archived_at  timestamptz,
    -- Higher first when campaigns overlap (then the latest start). Only one campaign shows at a time.
    priority     int NOT NULL DEFAULT 0,
    -- Greeting, template, colours, decorations, animation, height, button (validated by the server).
    design       jsonb NOT NULL,
    -- Featured products: automatic best sellers (period, category, number) or a manual list.
    products     jsonb NOT NULL,
    -- Pages and display style; visitors may dismiss it.
    placement    jsonb NOT NULL,
    version      int NOT NULL DEFAULT 1,
    created_by   uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CHECK (ends_at > starts_at)
);
CREATE INDEX website_campaigns_live_idx ON website_campaigns (tenant_id, starts_at, ends_at) WHERE published AND archived_at IS NULL;

-- Campaign insights from the existing anonymous website analytics (same consent rules).
ALTER TABLE website_events DROP CONSTRAINT website_events_kind_check;
ALTER TABLE website_events ADD CONSTRAINT website_events_kind_check
    CHECK (kind IN ('visit', 'product_view', 'add_to_cart', 'order_start', 'order_complete', 'campaign_view', 'campaign_product', 'campaign_cta'));
ALTER TABLE website_events ADD COLUMN campaign_id uuid REFERENCES website_campaigns(id) ON DELETE SET NULL;
CREATE INDEX website_events_campaign_idx ON website_events (campaign_id, at) WHERE campaign_id IS NOT NULL;

INSERT INTO platform_settings (key, value) VALUES ('campaigns', '{"enabled": true, "max_campaigns": 20, "max_products": 12}')
ON CONFLICT (key) DO NOTHING;
