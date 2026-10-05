-- Roadmap 14: finer access control.
-- Roles can be retired without deleting them (users must be moved first).
ALTER TABLE roles ADD COLUMN is_active boolean NOT NULL DEFAULT true;

-- New permissions on existing roles, keeping today's behaviour for managers and viewers:
-- anyone who could open dashboards or reports keeps seeing other employees' figures;
-- counter roles (e.g. Salesperson) now see only their own sales and performance.
UPDATE roles SET permissions = array_append(permissions, 'staff.view_others')
 WHERE NOT ('*' = ANY(permissions)) AND NOT ('staff.view_others' = ANY(permissions))
   AND ('dashboard.view' = ANY(permissions) OR 'reports.view' = ANY(permissions));
-- Everyone who sells or views sales keeps printing and sharing receipts.
UPDATE roles SET permissions = array_append(permissions, 'sales.print')
 WHERE NOT ('*' = ANY(permissions)) AND NOT ('sales.print' = ANY(permissions))
   AND ('sales.create' = ANY(permissions) OR 'sales.view' = ANY(permissions));
