-- Roadmap 17: geofencing. A branch may record where it is; when the business requires it (Settings → Workspace),
-- selected actions are accepted only from devices within the branch radius. Off by default everywhere.
ALTER TABLE branches ADD COLUMN latitude double precision CHECK (latitude BETWEEN -90 AND 90);
ALTER TABLE branches ADD COLUMN longitude double precision CHECK (longitude BETWEEN -180 AND 180);
ALTER TABLE branches ADD COLUMN geofence_radius_m int NOT NULL DEFAULT 150 CHECK (geofence_radius_m BETWEEN 20 AND 5000);
ALTER TABLE branches ADD COLUMN geofence_enabled boolean NOT NULL DEFAULT false;
ALTER TABLE branches ADD CONSTRAINT branches_geofence_needs_point CHECK (NOT geofence_enabled OR (latitude IS NOT NULL AND longitude IS NOT NULL));

-- Where the device reported itself when an audited action was taken ({lat, lng, accuracy_m}).
ALTER TABLE audit_log ADD COLUMN location jsonb;

-- Managers keep working away from the branch (deliveries, head office) when a business turns geofencing on.
UPDATE roles SET permissions = array_append(permissions, 'location.bypass')
 WHERE NOT ('*' = ANY(permissions)) AND NOT ('location.bypass' = ANY(permissions))
   AND 'sales.discount_override' = ANY(permissions) AND 'reports.view' = ANY(permissions);
