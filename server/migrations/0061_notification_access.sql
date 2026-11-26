-- Roadmap 194: notifications follow each person's access. Every notification records its category (one *Notifications*
-- permission each), the module permission its record needs and the branch it concerns; reading, counting and opening
-- apply the person's current access, so changes of role, permissions or branches take effect without deleting history.
ALTER TABLE notifications
    ADD COLUMN category text,
    ADD COLUMN perm text,
    ADD COLUMN branch_id uuid REFERENCES branches(id) ON DELETE SET NULL,
    ADD COLUMN mandatory boolean NOT NULL DEFAULT false;

UPDATE notifications SET
    category = CASE
        WHEN kind = 'new_order' THEN 'orders'
        WHEN kind = 'credit_overdue' THEN 'credit'
        WHEN kind IN ('low_stock', 'out_of_stock', 'product_deactivated') OR kind LIKE 'transfer\_%' THEN 'stock'
        WHEN kind = 'mpesa_attention' THEN 'sales'
        WHEN kind IN ('approval_pending', 'approval_handled') THEN 'approvals'
        WHEN kind = 'approval_decided' THEN 'own_requests'
        WHEN kind IN ('website_request', 'website_service', 'website_domain', 'article_review', 'article_ai_limit') THEN 'website'
        ELSE 'admin' END,
    perm = CASE
        WHEN kind = 'new_order' THEN 'orders.view'
        WHEN kind = 'credit_overdue' THEN 'credit.view'
        WHEN kind IN ('low_stock', 'out_of_stock') OR kind LIKE 'transfer\_%' THEN 'stock.view'
        WHEN kind = 'product_deactivated' THEN 'products.view'
        WHEN kind IN ('website_request', 'website_service', 'website_domain', 'article_review', 'article_ai_limit') THEN 'website.view'
        END,
    mandatory = kind IN ('support_request', 'support_started', 'support_decision', 'access_request', 'error_alert');

-- The branch of earlier notifications, where it can be recovered.
UPDATE notifications n SET branch_id = b.id FROM branches b
 WHERE n.kind IN ('low_stock', 'out_of_stock') AND n.dedupe_key ~ '^[a-z_]+:[0-9a-f-]{36}:' AND b.id::text = split_part(n.dedupe_key, ':', 2);
UPDATE notifications n SET branch_id = cs.branch_id FROM credit_sales cs
 WHERE n.kind = 'credit_overdue' AND n.dedupe_key = 'credit_overdue:' || cs.id::text;
UPDATE notifications n SET branch_id = o.branch_id FROM orders o
 WHERE n.kind = 'new_order' AND n.link = '/orders/' || o.id::text;

CREATE INDEX notifications_unread_idx ON notifications (user_id) WHERE read_at IS NULL;

-- Existing roles keep receiving what they received and keep seeing their own requests (roadmap 193); the Tenant
-- Administrator can now narrow both per role.
UPDATE roles SET permissions = ARRAY(SELECT DISTINCT unnest(permissions || ARRAY[
    'approvals.view_own', 'notify.sales', 'notify.credit', 'notify.stock', 'notify.customers', 'notify.expenses',
    'notify.orders', 'notify.approvals', 'notify.own_requests', 'notify.website', 'notify.admin']::text[]))
 WHERE NOT ('*' = ANY(permissions));
