-- Phase 9: imports, exports and mock data runs. Live progress is kept in
-- memory by the API; this table is the durable record of each job.

CREATE TABLE data_jobs (
    id              uuid PRIMARY KEY,
    user_id         uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    connection_id   uuid,
    kind            text NOT NULL,
    status          text NOT NULL,
    -- Human-readable target: `schema.table`, a file name or a table list.
    target          text,
    rows_processed  bigint NOT NULL DEFAULT 0,
    rows_failed     bigint NOT NULL DEFAULT 0,
    report          jsonb,
    started_at      timestamptz NOT NULL DEFAULT now(),
    finished_at     timestamptz,
    CONSTRAINT data_jobs_kind CHECK (kind IN ('import', 'mock_data', 'export')),
    CONSTRAINT data_jobs_status CHECK (status IN ('running', 'completed', 'failed', 'cancelled')),
    CONSTRAINT data_jobs_connection_fkey FOREIGN KEY (connection_id, user_id)
        REFERENCES database_connections (id, user_id) ON DELETE SET NULL (connection_id)
);
CREATE INDEX data_jobs_user_idx ON data_jobs (user_id, started_at DESC);

ALTER TABLE query_history
    DROP CONSTRAINT query_history_kind,
    ADD CONSTRAINT query_history_kind
        CHECK (kind IN ('query', 'data_apply', 'migration', 'import', 'mock_data', 'admin'));
