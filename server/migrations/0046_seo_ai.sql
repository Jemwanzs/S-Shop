-- Roadmap 159: AI-assisted article drafts. Off unless the platform owner enables it and the business opts in; every draft is
-- counted per business so usage and cost can be limited. No article text is kept here.
CREATE TABLE seo_automation (
    tenant_id        uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    enabled          boolean NOT NULL DEFAULT false,
    frequency        text NOT NULL DEFAULT 'weekly' CHECK (frequency IN ('weekly', 'biweekly', 'monthly')),
    -- Monday = 0 … Sunday = 6, at a local time of the business.
    weekday          int NOT NULL DEFAULT 0 CHECK (weekday BETWEEN 0 AND 6),
    at_time          text NOT NULL DEFAULT '09:00',
    industry         text NOT NULL DEFAULT '',
    categories       text[] NOT NULL DEFAULT '{}',
    audience         text NOT NULL DEFAULT '',
    area             text NOT NULL DEFAULT '',
    language         text NOT NULL DEFAULT '',
    tone             text NOT NULL DEFAULT '',
    length           text NOT NULL DEFAULT 'medium' CHECK (length IN ('short', 'medium', 'long')),
    link_products    boolean NOT NULL DEFAULT true,
    require_approval boolean NOT NULL DEFAULT true,
    next_run_at      timestamptz,
    last_run_at      timestamptz,
    attempts         int NOT NULL DEFAULT 0,
    updated_at       timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX seo_automation_due_idx ON seo_automation (next_run_at) WHERE enabled;

-- One row per request to the AI service. `run_key` makes a scheduled run idempotent.
CREATE TABLE seo_ai_usage (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    article_id    uuid REFERENCES website_articles(id) ON DELETE SET NULL,
    kind          text NOT NULL CHECK (kind IN ('assisted', 'scheduled')),
    run_key       text NOT NULL UNIQUE,
    status        text NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'ok', 'failed', 'refused', 'similar')),
    model         text NOT NULL DEFAULT '',
    input_tokens  int NOT NULL DEFAULT 0,
    output_tokens int NOT NULL DEFAULT 0,
    cost_usd      numeric(10, 4) NOT NULL DEFAULT 0,
    error         text NOT NULL DEFAULT '',
    created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX seo_ai_usage_month_idx ON seo_ai_usage (tenant_id, created_at DESC);

-- The platform owner's per-business limits (no row = the platform default).
CREATE TABLE seo_tenant_limits (
    tenant_id     uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    ai_enabled    boolean,
    monthly_quota int CHECK (monthly_quota >= 0),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
