-- Phase 10b: team workspaces, roles, per-connection grants, invitations and
-- snippets.
--
-- Every user gets a personal workspace. Connections, connection groups,
-- collections and saved queries belong to a workspace; history, plans,
-- migration projects and jobs stay personal but may reference shared
-- connections, so their connection foreign keys no longer include the
-- owner.

CREATE TABLE workspaces (
    id          uuid PRIMARY KEY,
    name        text NOT NULL,
    personal    boolean NOT NULL DEFAULT false,
    created_by  uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT workspaces_name_length CHECK (char_length(name) BETWEEN 1 AND 100)
);
-- One personal workspace per user.
CREATE UNIQUE INDEX workspaces_personal_key ON workspaces (created_by) WHERE personal;

CREATE TABLE workspace_members (
    workspace_id  uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role          text NOT NULL,
    joined_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id),
    CONSTRAINT workspace_members_role CHECK (role IN ('owner', 'admin', 'editor', 'viewer'))
);
CREATE INDEX workspace_members_user_idx ON workspace_members (user_id);

CREATE TABLE workspace_invitations (
    id           uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    email        text NOT NULL,
    role         text NOT NULL,
    token_hash   bytea NOT NULL UNIQUE,
    invited_by   uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    expires_at   timestamptz NOT NULL,
    accepted_at  timestamptz,
    accepted_by  uuid REFERENCES users (id) ON DELETE SET NULL,
    revoked_at   timestamptz,
    CONSTRAINT workspace_invitations_role CHECK (role IN ('admin', 'editor', 'viewer')),
    CONSTRAINT workspace_invitations_email_length CHECK (char_length(email) BETWEEN 3 AND 254)
);
CREATE INDEX workspace_invitations_ws_idx ON workspace_invitations (workspace_id, created_at DESC);

-- Personal workspaces for existing users.
INSERT INTO workspaces (id, name, personal, created_by)
SELECT gen_random_uuid(), 'Personal', true, id FROM users;
INSERT INTO workspace_members (workspace_id, user_id, role)
SELECT id, created_by, 'owner' FROM workspaces WHERE personal;

-- ---------------------------------------------------------------------------
-- Connection groups and connections move into workspaces
-- ---------------------------------------------------------------------------
ALTER TABLE connection_groups ADD COLUMN workspace_id uuid REFERENCES workspaces (id) ON DELETE CASCADE;
UPDATE connection_groups g SET workspace_id = w.id
FROM workspaces w WHERE w.personal AND w.created_by = g.user_id;
ALTER TABLE connection_groups
    ALTER COLUMN workspace_id SET NOT NULL,
    ADD CONSTRAINT connection_groups_id_workspace_key UNIQUE (id, workspace_id);
DROP INDEX connection_groups_user_name_key;
CREATE UNIQUE INDEX connection_groups_workspace_name_key ON connection_groups (workspace_id, lower(name));

ALTER TABLE database_connections ADD COLUMN workspace_id uuid REFERENCES workspaces (id) ON DELETE CASCADE;
UPDATE database_connections c SET workspace_id = w.id
FROM workspaces w WHERE w.personal AND w.created_by = c.user_id;
ALTER TABLE database_connections
    ALTER COLUMN workspace_id SET NOT NULL,
    DROP CONSTRAINT database_connections_user_name_key,
    ADD CONSTRAINT database_connections_workspace_name_key UNIQUE (workspace_id, name),
    DROP CONSTRAINT database_connections_group_fk,
    ADD CONSTRAINT database_connections_group_fk
        FOREIGN KEY (group_id, workspace_id) REFERENCES connection_groups (id, workspace_id)
        ON DELETE SET NULL (group_id);
CREATE INDEX database_connections_workspace_idx ON database_connections (workspace_id);

-- Explicit access of one member to one connection. Without a grant, access
-- follows the member's role.
CREATE TABLE connection_grants (
    connection_id  uuid NOT NULL REFERENCES database_connections (id) ON DELETE CASCADE,
    user_id        uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    access         text NOT NULL,
    updated_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (connection_id, user_id),
    CONSTRAINT connection_grants_access CHECK (access IN ('none', 'read', 'write', 'admin'))
);
CREATE INDEX connection_grants_user_idx ON connection_grants (user_id);

-- ---------------------------------------------------------------------------
-- Collections and saved queries move into workspaces
-- ---------------------------------------------------------------------------
ALTER TABLE query_collections ADD COLUMN workspace_id uuid REFERENCES workspaces (id) ON DELETE CASCADE;
UPDATE query_collections c SET workspace_id = w.id
FROM workspaces w WHERE w.personal AND w.created_by = c.user_id;
ALTER TABLE query_collections
    ALTER COLUMN workspace_id SET NOT NULL,
    ADD CONSTRAINT query_collections_id_workspace_key UNIQUE (id, workspace_id),
    DROP CONSTRAINT query_collections_parent_fkey,
    ADD CONSTRAINT query_collections_parent_fkey FOREIGN KEY (parent_id, workspace_id)
        REFERENCES query_collections (id, workspace_id) ON DELETE CASCADE;
DROP INDEX query_collections_sibling_name_key;
CREATE UNIQUE INDEX query_collections_sibling_name_key
    ON query_collections (workspace_id, parent_id, name) NULLS NOT DISTINCT;

ALTER TABLE saved_queries ADD COLUMN workspace_id uuid REFERENCES workspaces (id) ON DELETE CASCADE;
UPDATE saved_queries q SET workspace_id = w.id
FROM workspaces w WHERE w.personal AND w.created_by = q.user_id;
ALTER TABLE saved_queries
    ALTER COLUMN workspace_id SET NOT NULL,
    DROP CONSTRAINT saved_queries_collection_fkey,
    ADD CONSTRAINT saved_queries_collection_fkey FOREIGN KEY (collection_id, workspace_id)
        REFERENCES query_collections (id, workspace_id) ON DELETE SET NULL (collection_id),
    DROP CONSTRAINT saved_queries_connection_fkey,
    ADD CONSTRAINT saved_queries_connection_fkey FOREIGN KEY (connection_id)
        REFERENCES database_connections (id) ON DELETE SET NULL;
-- Saved queries keep their creator; deleting the account keeps shared work.
ALTER TABLE saved_queries ALTER COLUMN user_id DROP NOT NULL,
    DROP CONSTRAINT saved_queries_user_id_fkey,
    ADD CONSTRAINT saved_queries_user_id_fkey FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE SET NULL;
CREATE INDEX saved_queries_workspace_idx ON saved_queries (workspace_id, updated_at DESC);

-- ---------------------------------------------------------------------------
-- Personal records referencing (possibly shared) connections
-- ---------------------------------------------------------------------------
ALTER TABLE query_history
    DROP CONSTRAINT query_history_connection_fkey,
    ADD CONSTRAINT query_history_connection_fkey FOREIGN KEY (connection_id)
        REFERENCES database_connections (id) ON DELETE SET NULL;
ALTER TABLE saved_plans
    DROP CONSTRAINT saved_plans_connection_fkey,
    ADD CONSTRAINT saved_plans_connection_fkey FOREIGN KEY (connection_id)
        REFERENCES database_connections (id) ON DELETE SET NULL;
ALTER TABLE migration_runs
    DROP CONSTRAINT migration_runs_connection_fkey,
    ADD CONSTRAINT migration_runs_connection_fkey FOREIGN KEY (connection_id)
        REFERENCES database_connections (id) ON DELETE SET NULL;
ALTER TABLE data_jobs
    DROP CONSTRAINT data_jobs_connection_fkey,
    ADD CONSTRAINT data_jobs_connection_fkey FOREIGN KEY (connection_id)
        REFERENCES database_connections (id) ON DELETE SET NULL;
-- Diagram layouts are shared per connection; `user_id` is the last editor.
ALTER TABLE diagram_layouts
    DROP CONSTRAINT diagram_layouts_connection_fkey,
    ADD CONSTRAINT diagram_layouts_connection_fkey FOREIGN KEY (connection_id)
        REFERENCES database_connections (id) ON DELETE CASCADE,
    ADD CONSTRAINT diagram_layouts_user_fkey FOREIGN KEY (user_id)
        REFERENCES users (id) ON DELETE CASCADE;

-- ---------------------------------------------------------------------------
-- Snippets: reusable SQL templates with {{placeholders}}
-- ---------------------------------------------------------------------------
CREATE TABLE snippets (
    id            uuid PRIMARY KEY,
    workspace_id  uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    created_by    uuid REFERENCES users (id) ON DELETE SET NULL,
    name          text NOT NULL,
    description   text,
    body          text NOT NULL,
    tags          text[] NOT NULL DEFAULT '{}',
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT snippets_name_length CHECK (char_length(name) BETWEEN 1 AND 100),
    CONSTRAINT snippets_description_length CHECK (description IS NULL OR char_length(description) <= 500),
    CONSTRAINT snippets_body_size CHECK (octet_length(body) BETWEEN 1 AND 102400),
    CONSTRAINT snippets_tags_count CHECK (cardinality(tags) <= 20)
);
CREATE UNIQUE INDEX snippets_workspace_name_key ON snippets (workspace_id, lower(name));
CREATE TRIGGER snippets_touch BEFORE UPDATE ON snippets
    FOR EACH ROW EXECUTE FUNCTION querylab_touch_updated_at();
