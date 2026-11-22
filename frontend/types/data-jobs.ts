/** Imports, exports and mock data (mirrors `models::data_jobs`). */

export type JobKind = "import" | "mock_data" | "export";
export type JobStatus = "uploading" | "ready" | "running" | "completed" | "failed" | "cancelled";

export interface RowError {
  row: number;
  line: number | null;
  column: string | null;
  message: string;
}

export interface TableCount {
  table: string;
  requested: number;
  inserted: number;
}

export interface JobView {
  id: string;
  kind: JobKind;
  status: JobStatus;
  connection_id: string | null;
  target: string | null;
  phase: string | null;
  file_name: string | null;
  bytes_total: number;
  bytes_received: number;
  rows_total: number | null;
  rows_processed: number;
  rows_inserted: number;
  rows_updated: number;
  rows_skipped: number;
  rows_failed: number;
  errors: RowError[];
  tables: TableCount[];
  committed: boolean;
  truncated: boolean;
  error: string | null;
  created_at: string;
  started_at: string | null;
  finished_at: string | null;
  duration_ms: number | null;
}

export interface JobRecord {
  id: string;
  connection_id: string | null;
  connection_name: string | null;
  kind: JobKind;
  status: JobStatus;
  target: string | null;
  rows_processed: number;
  rows_failed: number;
  started_at: string;
  finished_at: string | null;
}

export type FileFormat = "auto" | "csv" | "json";

export interface ParseOptions {
  format?: FileFormat;
  delimiter?: string | null;
  has_header?: boolean;
}

export type InferredType =
  | "unknown"
  | "boolean"
  | "integer"
  | "decimal"
  | "date"
  | "timestamp"
  | "uuid"
  | "json"
  | "text";

export interface SourceColumn {
  index: number;
  name: string;
  inferred_type: InferredType;
  empty_count: number;
  max_length: number;
}

export interface ImportPreview {
  import_id: string;
  format: FileFormat;
  delimiter: string | null;
  has_header: boolean;
  columns: SourceColumn[];
  rows: (string | null)[][];
  total_rows: number;
  sampled_rows: number;
  parse_errors: RowError[];
  parse_error_count: number;
}

export interface ColumnMapping {
  source: number;
  target: string;
}

export interface TargetColumn {
  name: string;
  data_type: string;
  kind: string;
  nullable: boolean;
  has_default: boolean;
  identity: "always" | "by_default" | null;
  generated: boolean;
  enum_values: string[];
  required: boolean;
}

export interface MappingIssue {
  severity: "warning" | "error";
  source: number | null;
  target: string | null;
  message: string;
}

export interface CheckImportResponse {
  columns: TargetColumn[];
  mapping: ColumnMapping[];
  issues: MappingIssue[];
  unique_keys: string[][];
  can_insert: boolean;
  can_update: boolean;
  connection_read_only: boolean;
}

export type DuplicateMode = "error" | "skip" | "update";
export type ErrorMode = "all_or_nothing" | "skip_invalid";

export interface RunImportRequest {
  schema: string;
  table: string;
  mapping: ColumnMapping[];
  duplicate_mode: DuplicateMode;
  conflict_columns?: string[] | null;
  error_mode: ErrorMode;
  empty_as_null: boolean;
  confirm?: string;
}

export type Generator =
  | { kind: "default" }
  | { kind: "null" }
  | { kind: "constant"; value: string }
  | { kind: "first_name" }
  | { kind: "last_name" }
  | { kind: "full_name" }
  | { kind: "email" }
  | { kind: "username" }
  | { kind: "phone" }
  | { kind: "company" }
  | { kind: "product_name" }
  | { kind: "word" }
  | { kind: "sentence" }
  | { kind: "paragraph" }
  | { kind: "city" }
  | { kind: "country" }
  | { kind: "country_code" }
  | { kind: "street_address" }
  | { kind: "postal_code" }
  | { kind: "url" }
  | { kind: "uuid" }
  | { kind: "boolean"; true_ratio: number }
  | { kind: "integer"; min: number; max: number }
  | { kind: "sequence"; start?: number | null }
  | { kind: "decimal"; min: number; max: number; scale: number }
  | { kind: "date"; from: string; to: string }
  | { kind: "timestamp"; from: string; to: string }
  | { kind: "values"; values: string[] }
  | { kind: "json" }
  | { kind: "reference" };

export type GeneratorKind = Generator["kind"];

export interface MockTable {
  schema: string;
  table: string;
  rows: number;
  columns?: Record<string, Generator>;
}

export interface MockRequest {
  tables: MockTable[];
  seed?: number;
  confirm?: string;
}

export interface MockColumn {
  name: string;
  data_type: string;
  nullable: boolean;
  generator: Generator;
  references: string | null;
  note: string | null;
}

export interface MockTablePlan {
  schema: string;
  table: string;
  rows: number;
  columns: MockColumn[];
  sample: (string | null)[][];
  warnings: string[];
}

export interface MockPreview {
  seed: number;
  tables: MockTablePlan[];
  total_rows: number;
  missing_parents: string[];
  production_blocked: boolean;
}
