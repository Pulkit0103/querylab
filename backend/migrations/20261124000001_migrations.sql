-- Phase 8: migration projects, scripts and run history. The applied state
-- of a database lives in that database (querylab.schema_migrations); these
-- tables keep the scripts and who ran what, when.

CREATE TABLE migration_projects (
    id          uuid PRIMARY KEY,
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        text NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT migration_projects_name_length CHECK (char_length(name) BETWEEN 1 AND 100),
    CONSTRAINT migration_projects_user_name_key UNIQUE (user_id, name),
    CONSTRAINT migration_projects_id_user_key UNIQUE (id, user_id)
);

CREATE TABLE migrations (
    id          uuid PRIMARY KEY,
    project_id  uuid NOT NULL,
    user_id     uuid NOT NULL,
    version     bigint NOT NULL,
    name        text NOT NULL,
    up_sql      text NOT NULL,
    down_sql    text,
    -- SHA-256 of up_sql, hex.
    checksum    text NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT migrations_version_positive CHECK (version > 0),
    CONSTRAINT migrations_name_length CHECK (char_length(name) BETWEEN 1 AND 200),
    CONSTRAINT migrations_sql_size CHECK (octet_length(up_sql) <= 1048576
                                          AND octet_length(COALESCE(down_sql, '')) <= 1048576),
    CONSTRAINT migrations_project_version_key UNIQUE (project_id, version),
    CONSTRAINT migrations_id_user_key UNIQUE (id, user_id),
    CONSTRAINT migrations_project_fkey FOREIGN KEY (project_id, user_id)
        REFERENCES migration_projects (id, user_id) ON DELETE CASCADE
);

CREATE TABLE migration_runs (
    id             uuid PRIMARY KEY,
    migration_id   uuid NOT NULL,
    user_id        uuid NOT NULL,
    connection_id  uuid,
    direction      text NOT NULL,
    status         text NOT NULL,
    error_message  text,
    duration_ms    bigint NOT NULL DEFAULT 0,
    started_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT migration_runs_direction CHECK (direction IN ('up', 'down')),
    CONSTRAINT migration_runs_status CHECK (status IN ('success', 'error')),
    CONSTRAINT migration_runs_migration_fkey FOREIGN KEY (migration_id, user_id)
        REFERENCES migrations (id, user_id) ON DELETE CASCADE,
    CONSTRAINT migration_runs_connection_fkey FOREIGN KEY (connection_id, user_id)
        REFERENCES database_connections (id, user_id) ON DELETE SET NULL (connection_id)
);
CREATE INDEX migration_runs_migration_idx ON migration_runs (migration_id, started_at DESC);
CREATE INDEX migration_runs_user_idx ON migration_runs (user_id, started_at DESC);
