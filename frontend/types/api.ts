/** API contract types shared with the Rust backend (`backend/src/models`). */

export interface HealthResponse {
  status: "ok" | "degraded";
  version: string;
  database: "ok" | "unavailable";
}
