-- QueryLab application metadata schema.
-- Requires PostgreSQL 15+ (column lists in ON DELETE SET NULL).

CREATE OR REPLACE FUNCTION querylab_touch_updated_at() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

-- ---------------------------------------------------------------------------
-- Users and sessions
-- ---------------------------------------------------------------------------
CREATE TABLE users (
    id             uuid PRIMARY KEY,
    email          text NOT NULL,
    password_hash  text NOT NULL,
    display_name   text,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT users_email_length CHECK (char_length(email) BETWEEN 3 AND 254),
    CONSTRAINT users_display_name_length CHECK (display_name IS NULL OR char_length(display_name) <= 100)
);
CREATE UNIQUE INDEX users_email_lower_key ON users (lower(email));
CREATE TRIGGER users_touch BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION querylab_touch_updated_at();

CREATE TABLE sessions (
    id            uuid PRIMARY KEY,
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- SHA-256 of the opaque bearer token; the token itself is never stored.
    token_hash    bytea NOT NULL UNIQUE,
    created_at    timestamptz NOT NULL DEFAULT now(),
    last_seen_at  timestamptz NOT NULL DEFAULT now(),
    expires_at    timestamptz NOT NULL,
    user_agent    text,
    CONSTRAINT sessions_token_hash_length CHECK (octet_length(token_hash) = 32)
);
CREATE INDEX sessions_user_idx ON sessions (user_id);
CREATE INDEX sessions_expires_idx ON sessions (expires_at);

-- ---------------------------------------------------------------------------
-- Database connections (user-owned targets)
-- ---------------------------------------------------------------------------
CREATE TABLE database_connections (
    id                     uuid PRIMARY KEY,
    user_id                uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name                   text NOT NULL,
    database_type          text NOT NULL DEFAULT 'postgresql',
    host                   text NOT NULL,
    port                   integer NOT NULL,
    database_name          text NOT NULL,
    username               text NOT NULL,
    ssl_mode               text NOT NULL DEFAULT 'prefer',
    read_only              boolean NOT NULL DEFAULT true,
    -- AES-256-GCM: version || key id || nonce || ciphertext+tag
    encrypted_credentials  bytea NOT NULL,
    created_at             timestamptz NOT NULL DEFAULT now(),
    updated_at             timestamptz NOT NULL DEFAULT now(),
    last_connected_at      timestamptz,
    CONSTRAINT database_connections_name_length CHECK (char_length(name) BETWEEN 1 AND 100),
    CONSTRAINT database_connections_type CHECK (database_type IN ('postgresql')),
    CONSTRAINT database_connections_port CHECK (port BETWEEN 1 AND 65535),
    CONSTRAINT database_connections_ssl_mode
        CHECK (ssl_mode IN ('disable', 'prefer', 'require', 'verify-ca', 'verify-full')),
    CONSTRAINT database_connections_user_name_key UNIQUE (user_id, name),
    -- Target for composite foreign keys that enforce same-owner references.
    CONSTRAINT database_connections_id_user_key UNIQUE (id, user_id)
);
CREATE TRIGGER database_connections_touch BEFORE UPDATE ON database_connections
    FOR EACH ROW EXECUTE FUNCTION querylab_touch_updated_at();

-- ---------------------------------------------------------------------------
-- Saved queries and collections
-- ---------------------------------------------------------------------------
CREATE TABLE query_collections (
    id          uuid PRIMARY KEY,
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        text NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT query_collections_name_length CHECK (char_length(name) BETWEEN 1 AND 100),
    CONSTRAINT query_collections_user_name_key UNIQUE (user_id, name),
    CONSTRAINT query_collections_id_user_key UNIQUE (id, user_id)
);
CREATE TRIGGER query_collections_touch BEFORE UPDATE ON query_collections
    FOR EACH ROW EXECUTE FUNCTION querylab_touch_updated_at();

CREATE TABLE saved_queries (
    id             uuid PRIMARY KEY,
    user_id        uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    connection_id  uuid,
    collection_id  uuid,
    name           text NOT NULL,
    description    text,
    sql_content    text NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT saved_queries_name_length CHECK (char_length(name) BETWEEN 1 AND 200),
    CONSTRAINT saved_queries_description_length CHECK (description IS NULL OR char_length(description) <= 2000),
    CONSTRAINT saved_queries_sql_length CHECK (octet_length(sql_content) <= 1048576),
    -- Composite keys guarantee a saved query can only reference the owner's
    -- own connection/collection.
    CONSTRAINT saved_queries_connection_fkey FOREIGN KEY (connection_id, user_id)
        REFERENCES database_connections (id, user_id) ON DELETE SET NULL (connection_id),
    CONSTRAINT saved_queries_collection_fkey FOREIGN KEY (collection_id, user_id)
        REFERENCES query_collections (id, user_id) ON DELETE SET NULL (collection_id)
);
CREATE INDEX saved_queries_user_updated_idx ON saved_queries (user_id, updated_at DESC);
CREATE INDEX saved_queries_collection_idx ON saved_queries (collection_id) WHERE collection_id IS NOT NULL;
CREATE INDEX saved_queries_connection_idx ON saved_queries (connection_id) WHERE connection_id IS NOT NULL;
CREATE TRIGGER saved_queries_touch BEFORE UPDATE ON saved_queries
    FOR EACH ROW EXECUTE FUNCTION querylab_touch_updated_at();

-- ---------------------------------------------------------------------------
-- Query history
-- ---------------------------------------------------------------------------
CREATE TABLE query_history (
    id                 uuid PRIMARY KEY,
    user_id            uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    connection_id      uuid,
    sql_content        text NOT NULL,
    execution_time_ms  bigint NOT NULL,
    status             text NOT NULL,
    error_message      text,
    row_count          bigint,
    executed_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT query_history_status CHECK (status IN ('success', 'error', 'cancelled', 'timeout')),
    CONSTRAINT query_history_execution_time CHECK (execution_time_ms >= 0),
    CONSTRAINT query_history_sql_length CHECK (octet_length(sql_content) <= 1048576),
    CONSTRAINT query_history_connection_fkey FOREIGN KEY (connection_id, user_id)
        REFERENCES database_connections (id, user_id) ON DELETE SET NULL (connection_id)
);
CREATE INDEX query_history_user_executed_idx ON query_history (user_id, executed_at DESC, id DESC);
CREATE INDEX query_history_connection_idx ON query_history (connection_id) WHERE connection_id IS NOT NULL;
