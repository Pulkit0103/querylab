-- Phase 10a: account security (TOTP, password reset, session details) and
-- the audit log.

-- ---------------------------------------------------------------------------
-- Two-factor authentication
-- ---------------------------------------------------------------------------
ALTER TABLE users
    -- AES-256-GCM, bound to the user id. Set at enrolment; active once
    -- `totp_enabled_at` is set by a verified code.
    ADD COLUMN totp_secret_encrypted bytea,
    ADD COLUMN totp_enabled_at timestamptz,
    -- The last accepted TOTP time step, so a code cannot be replayed.
    ADD COLUMN totp_last_step bigint,
    ADD COLUMN password_changed_at timestamptz;

CREATE TABLE totp_recovery_codes (
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- SHA-256 of the normalised code.
    code_hash  bytea NOT NULL,
    used_at    timestamptz,
    PRIMARY KEY (user_id, code_hash)
);

-- The step between a correct password and a session for accounts with 2FA.
CREATE TABLE login_challenges (
    id          uuid PRIMARY KEY,
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash  bytea NOT NULL UNIQUE,
    expires_at  timestamptz NOT NULL,
    attempts    integer NOT NULL DEFAULT 0,
    user_agent  text,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX login_challenges_expires_idx ON login_challenges (expires_at);

-- ---------------------------------------------------------------------------
-- Password reset
-- ---------------------------------------------------------------------------
CREATE TABLE password_resets (
    id          uuid PRIMARY KEY,
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash  bytea NOT NULL UNIQUE,
    expires_at  timestamptz NOT NULL,
    used_at     timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX password_resets_user_idx ON password_resets (user_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- Sessions: where they were opened from
-- ---------------------------------------------------------------------------
ALTER TABLE sessions ADD COLUMN ip text;

-- ---------------------------------------------------------------------------
-- Audit log: insert-only for the application
-- ---------------------------------------------------------------------------
CREATE TABLE audit_events (
    id             bigserial PRIMARY KEY,
    workspace_id   uuid,
    actor_id       uuid,
    action         text NOT NULL,
    target_type    text,
    target_id      uuid,
    connection_id  uuid,
    detail         jsonb,
    ip             text,
    request_id     uuid,
    created_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT audit_events_action_length CHECK (char_length(action) BETWEEN 1 AND 100)
);
CREATE INDEX audit_events_ws_time_idx ON audit_events (workspace_id, created_at DESC);
CREATE INDEX audit_events_actor_time_idx ON audit_events (actor_id, created_at DESC);

-- Rows can never be changed. They can only be deleted once older than the
-- retention period the purge job sets for its own transaction.
CREATE FUNCTION querylab_audit_guard() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    retention text := current_setting('querylab.audit_retention_days', true);
BEGIN
    IF TG_OP = 'DELETE'
       AND retention ~ '^[0-9]+$' AND retention::int > 0
       AND OLD.created_at < now() - make_interval(days => retention::int) THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'audit events are append-only'
        USING ERRCODE = 'insufficient_privilege';
END;
$$;

CREATE TRIGGER audit_events_immutable
    BEFORE UPDATE OR DELETE ON audit_events
    FOR EACH ROW EXECUTE FUNCTION querylab_audit_guard();

-- TRUNCATE bypasses row triggers.
CREATE TRIGGER audit_events_no_truncate
    BEFORE TRUNCATE ON audit_events
    FOR EACH STATEMENT EXECUTE FUNCTION querylab_audit_guard();

REVOKE UPDATE, TRUNCATE ON audit_events FROM CURRENT_USER;
