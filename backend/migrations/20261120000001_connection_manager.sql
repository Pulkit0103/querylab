-- Phase 2: connection manager (groups, environments, status, custom CA).

CREATE TABLE connection_groups (
    id          uuid PRIMARY KEY,
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        text NOT NULL,
    position    integer NOT NULL DEFAULT 0,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT connection_groups_name_length CHECK (char_length(name) BETWEEN 1 AND 100),
    CONSTRAINT connection_groups_id_user_key UNIQUE (id, user_id)
);
CREATE UNIQUE INDEX connection_groups_user_name_key ON connection_groups (user_id, lower(name));
CREATE TRIGGER connection_groups_touch BEFORE UPDATE ON connection_groups
    FOR EACH ROW EXECUTE FUNCTION querylab_touch_updated_at();

ALTER TABLE database_connections
    ADD COLUMN environment text NOT NULL DEFAULT 'development',
    ADD COLUMN group_id uuid,
    ADD COLUMN color text,
    -- PEM-encoded CA certificate(s) for verify-ca / verify-full. Public data.
    ADD COLUMN ssl_root_cert text,
    -- Result of the latest user-initiated connection test.
    ADD COLUMN last_status text,
    ADD COLUMN last_status_at timestamptz,
    ADD COLUMN last_latency_ms integer,
    ADD COLUMN last_error text,
    ADD CONSTRAINT database_connections_environment_check
        CHECK (environment IN ('development', 'staging', 'production')),
    ADD CONSTRAINT database_connections_color_check
        CHECK (color IS NULL OR color ~ '^#[0-9a-f]{6}$'),
    ADD CONSTRAINT database_connections_ssl_root_cert_length
        CHECK (ssl_root_cert IS NULL OR char_length(ssl_root_cert) <= 65536),
    ADD CONSTRAINT database_connections_last_status_check
        CHECK (last_status IS NULL OR last_status IN ('online', 'unreachable', 'auth_failed', 'tls_error', 'error')),
    ADD CONSTRAINT database_connections_last_error_length
        CHECK (last_error IS NULL OR char_length(last_error) <= 500),
    -- A connection can only be filed under a group of the same user.
    ADD CONSTRAINT database_connections_group_fk
        FOREIGN KEY (group_id, user_id) REFERENCES connection_groups (id, user_id)
        ON DELETE SET NULL (group_id);

CREATE INDEX database_connections_group_idx ON database_connections (group_id) WHERE group_id IS NOT NULL;
