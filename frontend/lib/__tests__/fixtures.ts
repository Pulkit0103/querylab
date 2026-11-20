import type { Connection } from "@/types/api";

/** Builds a connection with sensible defaults for tests. */
export function conn(overrides: Partial<Connection>): Connection {
  return {
    id: "c",
    name: "conn",
    database_type: "postgresql",
    host: "db.example.com",
    port: 5432,
    database: "app",
    username: "reader",
    ssl_mode: "require",
    read_only: true,
    created_at: "2026-11-20T00:00:00Z",
    updated_at: "2026-11-20T00:00:00Z",
    last_connected_at: null,
    environment: "development",
    group_id: null,
    color: null,
    ssl_root_cert: null,
    last_status: null,
    last_status_at: null,
    last_latency_ms: null,
    last_error: null,
    ...overrides,
  };
}
