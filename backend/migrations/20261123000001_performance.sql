-- Phase 7: query fingerprints, audited admin actions and saved plans.

-- Normalised-statement hash (see utils::fingerprint). Existing rows are
-- filled in by the API at startup, in batches.
ALTER TABLE query_history ADD COLUMN fingerprint bigint;
CREATE INDEX query_history_fingerprint_idx
    ON query_history (user_id, fingerprint, executed_at) WHERE fingerprint IS NOT NULL;

-- Cancelling or terminating a backend is recorded as an `admin` entry.
ALTER TABLE query_history
    DROP CONSTRAINT query_history_kind,
    ADD CONSTRAINT query_history_kind
        CHECK (kind IN ('query', 'data_apply', 'migration', 'import', 'admin'));

-- Execution plans kept for side-by-side comparison. `plan` is the EXPLAIN
-- result as returned by /api/query/explain.
CREATE TABLE saved_plans (
    id             uuid PRIMARY KEY,
    user_id        uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    connection_id  uuid,
    name           text NOT NULL,
    sql_content    text NOT NULL,
    plan           jsonb NOT NULL,
    analyzed       boolean NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT saved_plans_connection_fkey FOREIGN KEY (connection_id, user_id)
        REFERENCES database_connections (id, user_id) ON DELETE SET NULL (connection_id),
    CONSTRAINT saved_plans_name_length CHECK (char_length(name) BETWEEN 1 AND 200),
    CONSTRAINT saved_plans_plan_object CHECK (jsonb_typeof(plan) = 'object'),
    CONSTRAINT saved_plans_plan_size CHECK (octet_length(plan::text) <= 2097152)
);
CREATE INDEX saved_plans_user_idx ON saved_plans (user_id, created_at DESC);
