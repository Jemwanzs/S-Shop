-- Roadmap 158: a business's own articles on its website. Each article belongs to one business; nothing is shared.
CREATE TABLE website_articles (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    slug           text NOT NULL,
    -- Addresses it was published under before a rename: they forward to the current one.
    previous_slugs text[] NOT NULL DEFAULT '{}',
    title          text NOT NULL,
    seo_title      text NOT NULL DEFAULT '',
    description    text NOT NULL DEFAULT '',
    -- The article's text: headings, paragraphs, lists, bold and links (see routes/articles.rs). Never raw HTML.
    body           text NOT NULL DEFAULT '',
    cover_media    uuid REFERENCES website_media(id) ON DELETE SET NULL,
    cover_alt      text NOT NULL DEFAULT '',
    category       text NOT NULL DEFAULT '',
    author_name    text NOT NULL DEFAULT '',
    product_ids    uuid[] NOT NULL DEFAULT '{}',
    status         text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'pending_review', 'scheduled', 'published', 'archived')),
    scheduled_at   timestamptz,
    published_at   timestamptz,
    -- Drafted with AI assistance (roadmap 159); said on the page once a person has reviewed and published it.
    ai_assisted    boolean NOT NULL DEFAULT false,
    reviewed_by    text NOT NULL DEFAULT '',
    views          bigint NOT NULL DEFAULT 0,
    created_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, slug)
);
CREATE INDEX website_articles_list_idx ON website_articles (tenant_id, status, published_at DESC);
CREATE INDEX website_articles_due_idx ON website_articles (scheduled_at) WHERE status = 'scheduled';
