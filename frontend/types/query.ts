/** Query execution contract (mirrors backend `models::query`). */

export interface ColumnMeta {
  name: string;
  /** PostgreSQL type name, e.g. `int4`, `text`, `timestamptz`. */
  type: string;
}

export type CellValue = string | number | boolean | null | Record<string, unknown> | unknown[];

export interface QueryResult {
  status: "success";
  columns: ColumnMeta[];
  rows: CellValue[][];
  /** Number of rows returned to the client. */
  row_count: number;
  /** Rows affected by INSERT/UPDATE/DELETE, when applicable. */
  rows_affected: number | null;
  /** True when the server stopped reading rows at the configured limit. */
  truncated: boolean;
  execution_time_ms: number;
  /** Leading SQL keyword reported by the server, e.g. `SELECT`. */
  command: string | null;
  history_id?: string | null;
}

/** Structured SQL error returned in `error.details` for failed executions. */
export interface SqlErrorDetails {
  sqlstate?: string | null;
  position?: number | null;
  detail?: string | null;
  hint?: string | null;
  execution_time_ms?: number;
}

export interface PlanNode {
  "Node Type": string;
  "Relation Name"?: string;
  "Schema"?: string;
  "Alias"?: string;
  "Index Name"?: string;
  "Join Type"?: string;
  "Startup Cost"?: number;
  "Total Cost"?: number;
  "Plan Rows"?: number;
  "Plan Width"?: number;
  "Actual Startup Time"?: number;
  "Actual Total Time"?: number;
  "Actual Rows"?: number;
  "Actual Loops"?: number;
  "Filter"?: string;
  "Index Cond"?: string;
  "Hash Cond"?: string;
  "Join Filter"?: string;
  "Sort Key"?: string[];
  "Rows Removed by Filter"?: number;
  Plans?: PlanNode[];
  [key: string]: unknown;
}

export interface ExplainResult {
  plan: PlanNode;
  planning_time_ms: number | null;
  execution_time_ms: number | null;
  analyzed: boolean;
  raw: unknown;
}
