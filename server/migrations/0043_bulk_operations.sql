-- Roadmap 151: bulk operations. Requests created together (a bulk product deactivation) share one reference, and a
-- request can carry the requester's reason. Decisions made together carry their bulk reference in `decisions`.
ALTER TABLE approvals ADD COLUMN bulk_ref uuid;
ALTER TABLE approvals ADD COLUMN note text NOT NULL DEFAULT '';
CREATE INDEX approvals_bulk_ref_idx ON approvals (bulk_ref) WHERE bulk_ref IS NOT NULL;
