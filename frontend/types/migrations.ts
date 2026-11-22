/** Schema diff and migrations (mirrors `models::schema_diff` / `models::migrations`). */
import type { SqlErrorDetails } from "@/types/query";

export type ChangePhase =
  | "drop_trigger"
  | "drop_view"
  | "drop_function"
  | "drop_foreign_key"
  | "drop_constraint"
  | "drop_index"
  | "drop_column"
  | "drop_table"
  | "drop_sequence"
  | "drop_type"
  | "create_schema"
  | "create_type"
  | "create_sequence"
  | "create_table"
  | "alter_column"
  | "add_constraint"
  | "add_foreign_key"
  | "create_index"
  | "sequence_owner"
  | "create_function"
  | "create_view"
  | "create_trigger";

export type ObjectType =
  | "schema"
  | "type"
  | "sequence"
  | "table"
  | "column"
  | "constraint"
  | "index"
  | "view"
  | "function"
  | "trigger";

export interface SchemaChange {
  phase: ChangePhase;
  object_type: ObjectType;
  action: "create" | "drop" | "alter";
  object: string;
  sql: string;
  down_sql: string | null;
  destructive: boolean;
  notes: string[];
  before: string | null;
  after: string | null;
}

export interface DiffResponse {
  source_schema: string;
  target_schema: string;
  preamble: string;
  changes: SchemaChange[];
  hints: string[];
}

export interface DiffSide {
  connection_id: string;
  schema: string;
}

export interface MigrationProject {
  id: string;
  name: string;
  created_at: string;
  migrations: number;
}

export interface Migration {
  id: string;
  project_id: string;
  version: number;
  name: string;
  up_sql: string;
  down_sql: string | null;
  checksum: string;
  created_at: string;
  updated_at: string;
  /** Applied somewhere: no longer editable. */
  locked: boolean;
}

export interface MigrationInput {
  version?: number;
  name: string;
  up_sql: string;
  down_sql?: string | null;
}

export type MigrationState = "applied" | "pending" | "drift";

export interface MigrationStatus {
  id: string;
  version: number;
  name: string;
  state: MigrationState;
  applied_at: string | null;
  applied_by: string | null;
  duration_ms: number | null;
}

export interface AppliedRow {
  version: number;
  name: string;
  checksum: string;
  applied_at: string;
  applied_by: string;
  duration_ms: number;
}

export interface StatusResponse {
  connection_id: string;
  tracking_table: boolean;
  note: string | null;
  migrations: MigrationStatus[];
  unknown: AppliedRow[];
}

export interface RunOutcome {
  dry_run: boolean;
  direction: "up" | "down";
  status: "success" | "error";
  duration_ms: number;
  error: (SqlErrorDetails & { message: string }) | null;
  run_id: string | null;
}

export interface MigrationRun {
  id: string;
  migration_id: string;
  project_id: string;
  version: number;
  name: string;
  connection_id: string | null;
  connection_name: string | null;
  direction: "up" | "down";
  status: "success" | "error";
  error_message: string | null;
  duration_ms: number;
  started_at: string;
}
