-- Roadmap 103: imports post in the background with real progress.
-- 'posting' = the import is running (one transaction: everything or nothing).
ALTER TABLE import_batches DROP CONSTRAINT import_batches_status_check;
ALTER TABLE import_batches ADD CONSTRAINT import_batches_status_check
    CHECK (status IN ('validated', 'pending_approval', 'posting', 'posted', 'rejected', 'cancelled', 'failed'));
-- Import reference number shown on the summary and the report.
ALTER TABLE import_batches ADD COLUMN reference text;
ALTER TABLE import_batches ADD COLUMN started_at timestamptz;

-- Progress lives apart from the batch row: the import's transaction holds the batch row locked until it commits,
-- so progress is written on its own connection without waiting on it. updated_at is a heartbeat while it runs.
CREATE TABLE import_progress (
    batch_id   uuid PRIMARY KEY REFERENCES import_batches(id) ON DELETE CASCADE,
    stage      text NOT NULL DEFAULT 'preparing' CHECK (stage IN ('preparing', 'validating', 'importing', 'finalizing', 'completed', 'failed')),
    done       integer NOT NULL DEFAULT 0,
    total      integer NOT NULL DEFAULT 0,
    updated_at timestamptz NOT NULL DEFAULT now()
);
