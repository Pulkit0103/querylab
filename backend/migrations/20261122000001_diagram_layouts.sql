-- Phase 6: saved ER diagram node positions per connection and schema.
-- Positions are a JSON object keyed by table name: {"orders": {"x": 0, "y": 0}}.
CREATE TABLE diagram_layouts (
    user_id        uuid NOT NULL,
    connection_id  uuid NOT NULL,
    schema_name    text NOT NULL,
    positions      jsonb NOT NULL,
    updated_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (connection_id, schema_name),
    CONSTRAINT diagram_layouts_connection_fkey FOREIGN KEY (connection_id, user_id)
        REFERENCES database_connections (id, user_id) ON DELETE CASCADE,
    CONSTRAINT diagram_layouts_positions_object CHECK (jsonb_typeof(positions) = 'object'),
    CONSTRAINT diagram_layouts_size CHECK (octet_length(positions::text) <= 1048576)
);
