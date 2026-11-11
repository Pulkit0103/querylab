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
}

export interface FieldError {
  field: string;
  message: string;
}

export type SslMode = "disable" | "prefer" | "require" | "verify-ca" | "verify-full";

export interface Connection {
  id: string;
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
}

export interface ConnectionInput {
  name: string;
  host: string;
  port: number;
  database: string;
  username: string;
  /** Omit on update to keep the stored password. */
  password?: string;
  ssl_mode: SslMode;
  read_only: boolean;
}
