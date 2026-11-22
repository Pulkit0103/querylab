/** API contract types shared with the Rust backend (`backend/src/models`). */

export interface HealthResponse {
  status: "ok" | "degraded";
  version: string;
  database: "ok" | "unavailable";
}

export interface User {
  id: string;
  email: string;
  display_name: string | null;
  created_at: string;
  two_factor_enabled: boolean;
}

export interface FieldError {
  field: string;
  message: string;
}

export type SslMode = "disable" | "prefer" | "require" | "verify-ca" | "verify-full";

export type Environment = "development" | "staging" | "production";

/** Outcome of the latest connection test (`null` until tested). */
export type ConnectionStatus = "online" | "unreachable" | "auth_failed" | "tls_error" | "error";

/** The caller's access to a connection. */
export type ConnectionAccess = "read" | "write" | "admin";

export interface Connection {
  id: string;
  workspace_id: string;
  access: ConnectionAccess;
  name: string;
  database_type: "postgresql";
  host: string;
  port: number;
  database: string;
  username: string;
  ssl_mode: SslMode;
  read_only: boolean;
  created_at: string;
  updated_at: string;
  last_connected_at: string | null;
  environment: Environment;
  group_id: string | null;
  /** `#rrggbb` label colour. */
  color: string | null;
  /** PEM CA bundle used with verify-ca / verify-full. */
  ssl_root_cert: string | null;
  last_status: ConnectionStatus | null;
  last_status_at: string | null;
  last_latency_ms: number | null;
  last_error: string | null;
}

export interface ConnectionInput {
  workspace_id?: string | null;
  name: string;
  host: string;
  port: number;
  database: string;
  username: string;
  /** Omit on update to keep the stored password. */
  password?: string;
  ssl_mode: SslMode;
  read_only: boolean;
  environment: Environment;
  /** `null` clears on update; omit to keep. */
  group_id?: string | null;
  color?: string | null;
  ssl_root_cert?: string | null;
}

export interface ConnectionGroup {
  id: string;
  name: string;
  position: number;
  connection_count: number;
  created_at: string;
  updated_at: string;
}
