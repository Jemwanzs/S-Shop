-- Roadmap 2: multi-level approval chains and conditional workflow rules.

-- Ordered approval levels: [{approver_type, approver_role_id, approver_user_id}, …]
ALTER TABLE workflows ADD COLUMN levels jsonb NOT NULL DEFAULT '[{"approver_type":"admin"}]'::jsonb;
-- When the rule applies (empty list = any): {"branch_ids":[], "role_ids":[], "category_ids":[]}
ALTER TABLE workflows ADD COLUMN conditions jsonb NOT NULL DEFAULT '{}'::jsonb;

-- The single approver configured so far becomes level 1.
UPDATE workflows SET levels = jsonb_build_array(jsonb_strip_nulls(jsonb_build_object(
    'approver_type', approver_type,
    'approver_role_id', approver_role_id,
    'approver_user_id', approver_user_id)));

ALTER TABLE workflows DROP COLUMN approver_type, DROP COLUMN approver_role_id, DROP COLUMN approver_user_id;

-- Progress of a request through its levels, and every decision taken so far.
ALTER TABLE approvals ADD COLUMN level int NOT NULL DEFAULT 1;
ALTER TABLE approvals ADD COLUMN decisions jsonb NOT NULL DEFAULT '[]'::jsonb;
