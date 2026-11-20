/** Data Studio API types (see backend `models/data.rs`). */

export type FilterOperator =
  | "="
  | "!="
  | ">"
  | ">="
  | "<"
  | "<="
  | "like"
  | "ilike"
  | "in"
  | "is_null"
  | "is_not_null"
  | "between";

export interface FilterCondition {
  column: string;
  operator: FilterOperator;
  value?: string | string[] | null;
}

export interface FilterGroup {
  op: "and" | "or";
  items: (FilterCondition | FilterGroup)[];
}

export interface SortKey {
  column: string;
  direction: "asc" | "desc";
}

export type ColumnKind =
  | "boolean"
  | "number"
  | "datetime"
  | "text"
  | "json"
  | "uuid"
  | "enum"
  | "array"
  | "binary"
  | "other";

export interface DataColumn {
  name: string;
  data_type: string;
  type_name: string;
  kind: ColumnKind;
  nullable: boolean;
  has_default: boolean;
  is_key: boolean;
  identity: "always" | "by_default" | null;
  generated: boolean;
  editable: boolean;
  enum_values: string[];
}

/** Cells are PostgreSQL text representations, or null. */
export type DataCell = string | null;

export interface BrowseRequest {
  schema: string;
  table: string;
  filter?: FilterGroup | null;
  sort?: SortKey[];
  search?: string;
  page_size?: number;
  cursor?: string | null;
}

export interface BrowseResponse {
  schema: string;
  table: string;
  kind: string;
  columns: DataColumn[];
  key_columns: string[];
  key_kind: "primary_key" | "unique" | null;
  rows: DataCell[][];
  pagination: "keyset" | "offset";
  next_cursor: string | null;
  total_rows: number | null;
  total_exact: boolean;
  editable: boolean;
  read_only_reason: string | null;
  can_insert: boolean;
  can_update: boolean;
  can_delete: boolean;
  sql: string;
  truncated: boolean;
  execution_time_ms: number;
}

export type ValueMap = Record<string, DataCell>;

export interface ChangeSet {
  inserts: { values: ValueMap }[];
  updates: { key: ValueMap; values: ValueMap; original?: ValueMap }[];
  deletes: { key: ValueMap }[];
}

export type ChangeKind = "insert" | "update" | "delete";

export interface PreviewResponse {
  select_sql: string;
  statements: { kind: ChangeKind; index: number; sql: string }[];
}

export interface SqlErrorDetails {
  message: string;
  sqlstate: string | null;
  detail: string | null;
  hint: string | null;
}

export interface ApplyResponse {
  committed: boolean;
  applied: { kind: ChangeKind; index: number; row: DataCell[] | null }[];
  failure: {
    kind: ChangeKind | null;
    index: number | null;
    conflict: boolean;
    message: string;
    sql_error: SqlErrorDetails | null;
    statement: string;
  } | null;
  rows_affected: number;
  execution_time_ms: number;
  history_id: string | null;
}

export type ExportFormat = "csv" | "json" | "sql";
