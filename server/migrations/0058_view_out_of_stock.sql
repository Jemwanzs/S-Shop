-- Roadmap 186: *View out-of-stock products when recording sales* (`sales.view_out_of_stock`). Every role that records
-- sales keeps today's behaviour (out-of-stock products shown, never sellable) — except the Salesperson role, which from
-- now on sees only products with stock at its branch, as the owner specified. Administrators change it per role.
UPDATE roles SET permissions = array_append(permissions, 'sales.view_out_of_stock')
 WHERE 'sales.create' = ANY(permissions) AND NOT ('sales.view_out_of_stock' = ANY(permissions)) AND name <> 'Salesperson';
