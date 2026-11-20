-- Phase 5: history filters and pins, tags, nested folders, tab sessions and
-- per-user history retention.

-- ---------------------------------------------------------------------------
-- Query history
-- ---------------------------------------------------------------------------
ALTER TABLE query_history
    ADD COLUMN pinned boolean NOT NULL DEFAULT false,
    ADD COLUMN kind   text    NOT NULL DEFAULT 'query',
    ADD CONSTRAINT query_history_kind CHECK (kind IN ('query', 'data_apply', 'migration', 'import'));

-- Data Studio applies recorded before `kind` existed start with this marker.
UPDATE query_history SET kind = 'data_apply' WHERE sql_content LIKE '-- Data Studio:%';

CREATE INDEX query_history_duration_idx ON query_history (user_id, execution_time_ms);
CREATE INDEX query_history_pinned_idx ON query_history (user_id, executed_at DESC, id DESC) WHERE pinned;

-- ---------------------------------------------------------------------------
-- Saved queries: pins and tags
-- ---------------------------------------------------------------------------
ALTER TABLE saved_queries
    ADD COLUMN pinned boolean NOT NULL DEFAULT false,
    ADD COLUMN tags   text[]  NOT NULL DEFAULT '{}',
    ADD CONSTRAINT saved_queries_tags_count CHECK (cardinality(tags) <= 20);
CREATE INDEX saved_queries_tags_idx ON saved_queries USING gin (tags);

-- Pinning is not an edit: only content changes move `updated_at`.
CREATE FUNCTION querylab_touch_saved_query() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF ROW(NEW.name, NEW.description, NEW.sql_content, NEW.connection_id, NEW.collection_id, NEW.tags)
       IS DISTINCT FROM
       ROW(OLD.name, OLD.description, OLD.sql_content, OLD.connection_id, OLD.collection_id, OLD.tags) THEN
        NEW.updated_at := now();
    END IF;
    RETURN NEW;
END;
$$;
DROP TRIGGER saved_queries_touch ON saved_queries;
CREATE TRIGGER saved_queries_touch BEFORE UPDATE ON saved_queries
    FOR EACH ROW EXECUTE FUNCTION querylab_touch_saved_query();

-- ---------------------------------------------------------------------------
-- Collections become folders. Names are unique among siblings rather than
-- per user; deleting a folder deletes its subfolders (queries are kept and
-- become unfiled through saved_queries_collection_fkey).
-- ---------------------------------------------------------------------------
ALTER TABLE query_collections
    ADD COLUMN parent_id uuid,
    ADD CONSTRAINT query_collections_parent_fkey FOREIGN KEY (parent_id, user_id)
        REFERENCES query_collections (id, user_id) ON DELETE CASCADE,
    ADD CONSTRAINT query_collections_not_own_parent CHECK (parent_id IS NULL OR parent_id <> id),
    DROP CONSTRAINT query_collections_user_name_key;
CREATE UNIQUE INDEX query_collections_sibling_name_key
    ON query_collections (user_id, parent_id, name) NULLS NOT DISTINCT;
CREATE INDEX query_collections_parent_idx ON query_collections (parent_id) WHERE parent_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- Per-user settings and the synced editor session
-- ---------------------------------------------------------------------------
ALTER TABLE users
    ADD COLUMN history_retention_days integer NOT NULL DEFAULT 90,
    ADD CONSTRAINT users_history_retention_days CHECK (history_retention_days BETWEEN 1 AND 3650);

CREATE TABLE workspace_sessions (
    user_id     uuid PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    session     jsonb NOT NULL,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT workspace_sessions_size CHECK (octet_length(session::text) <= 2097152)
);
