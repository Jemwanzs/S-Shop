-- Roadmap 194 follow-up: "Nobody can approve" alerts are updates on one's own requests (they reach the requester wherever
-- the request's branch is), not administrative notices.
UPDATE notifications SET category = 'own_requests', branch_id = NULL WHERE kind = 'approval_stuck';
