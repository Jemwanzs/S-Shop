-- Roadmap 62–64: sale ownership (owner vs recorder), controlled ownership changes, role / user data-visibility scopes.

-- The employee credited with the sale. `user_id` stays the person who recorded it (Recorded By).
ALTER TABLE sales ADD COLUMN owner_id uuid REFERENCES users(id) ON DELETE SET NULL;
UPDATE sales SET owner_id = user_id;
CREATE INDEX sales_owner_idx ON sales (tenant_id, owner_id, business_date);

-- Ownership change requests and their outcome (the approval itself lives in `approvals`).
CREATE TABLE sale_owner_changes (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    sale_id      uuid NOT NULL REFERENCES sales(id),
    from_owner   uuid REFERENCES users(id) ON DELETE SET NULL,
    to_owner     uuid REFERENCES users(id) ON DELETE SET NULL,
    reason       text NOT NULL,
    status       text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected', 'withdrawn')),
    approval_id  uuid REFERENCES approvals(id) ON DELETE SET NULL,
    requested_by uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_by   uuid REFERENCES users(id) ON DELETE SET NULL,
    decided_at   timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now()
);
-- One open request per sale.
CREATE UNIQUE INDEX sale_owner_changes_pending_uq ON sale_owner_changes (sale_id) WHERE status = 'pending';
CREATE INDEX sale_owner_changes_sale_idx ON sale_owner_changes (sale_id, created_at DESC);

-- Default workflow: Sale Ownership Change → Tenant Administrator (one level, enabled).
INSERT INTO workflows (tenant_id, action, enabled, levels)
SELECT id, 'sale.owner_change', true, jsonb_build_array(jsonb_build_object('id', gen_random_uuid(), 'approver_type', 'admin'))
FROM tenants
ON CONFLICT (tenant_id, action) DO NOTHING;

-- A user's default branch (used as the Current Branch after sign-in when they have several).
ALTER TABLE users ADD COLUMN default_branch_id uuid REFERENCES branches(id) ON DELETE SET NULL;

-- User-specific access exceptions: website permissions (roadmap 52), the sales-ownership and analytics permissions,
-- data scopes (scope.<area>.<own|branches|all>), and explicit restrictions written as "-<permission>".
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_extra_permissions_check;
ALTER TABLE users ADD CONSTRAINT users_extra_permissions_check CHECK (extra_permissions <@ ARRAY[
    'website.view', 'website.content', 'website.products', 'website.photos', 'website.categories', 'website.media',
    'website.services', 'website.testimonials', 'website.design', 'website.navigation', 'website.seo', 'website.domain',
    'website.preview', 'website.publish', 'website.analytics',
    'sales.assign_owner', 'sales.request_owner_change', 'dashboard.view', 'reports.view', 'reports.export', 'staff.view_others',
    '-sales.assign_owner', '-sales.request_owner_change', '-dashboard.view', '-reports.view', '-reports.export', '-staff.view_others',
    'scope.sales.own', 'scope.sales.branches', 'scope.sales.all',
    'scope.dashboard.own', 'scope.dashboard.branches', 'scope.dashboard.all',
    'scope.reports.own', 'scope.reports.branches', 'scope.reports.all',
    'scope.leaderboards.own', 'scope.leaderboards.branches', 'scope.leaderboards.all',
    'scope.orders.own', 'scope.orders.branches', 'scope.orders.all',
    'scope.credit.own', 'scope.credit.branches', 'scope.credit.all'
]::text[]);

-- Effective permissions: role ∪ user grants, minus user restrictions ("-x"); a user's scope for an area replaces the
-- role's scope for that area. Used everywhere permissions are resolved, so no screen or query can disagree.
CREATE FUNCTION effective_permissions(role_perms text[], extra text[]) RETURNS text[]
LANGUAGE sql IMMUTABLE AS $$
    SELECT COALESCE(array_agg(DISTINCT p ORDER BY p), '{}')
    FROM unnest(role_perms || ARRAY(SELECT e FROM unnest(extra) e WHERE e NOT LIKE '-%')) AS p
    WHERE NOT (('-' || p) = ANY(extra))
      AND NOT (p LIKE 'scope.%' AND NOT (p = ANY(extra))
               AND EXISTS (SELECT 1 FROM unnest(extra) e WHERE e LIKE 'scope.' || split_part(p, '.', 2) || '.%'))
$$;

-- Built-in roles get the new sales-ownership permissions (businesses can change them in Roles & permissions).
UPDATE roles SET permissions = permissions || ARRAY['sales.assign_owner', 'sales.request_owner_change']
WHERE is_system AND name IN ('Manager', 'Supervisor', 'Branch Manager') AND NOT ('sales.assign_owner' = ANY(permissions));
UPDATE roles SET permissions = permissions || ARRAY['sales.request_owner_change']
WHERE is_system AND name = 'Salesperson' AND NOT ('sales.request_owner_change' = ANY(permissions));
