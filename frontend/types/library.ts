/** History, saved queries, collections and stats (mirrors `models::library`). */

export type HistoryStatus = "success" | "error" | "cancelled" | "timeout";
export type HistoryKind = "query" | "data_apply" | "migration" | "import" | "mock_data" | "admin";

export interface HistoryEntry {
  id: string;
  connection_id: string | null;
  connection_name: string | null;
  sql_content: string;
  execution_time_ms: number;
  status: HistoryStatus;
  error_message: string | null;
  row_count: number | null;
  executed_at: string;
  pinned: boolean;
  kind: HistoryKind;
}

export interface HistoryFilters {
  q?: string;
  status?: HistoryStatus;
  kind?: HistoryKind;
  connection_id?: string;
  min_ms?: number;
  max_ms?: number;
  /** ISO timestamps; `to` is exclusive. */
  from?: string;
  to?: string;
  pinned?: boolean;
}

export interface HistoryPage {
  items: HistoryEntry[];
  next_cursor: string | null;
}

/** A folder of saved queries; folders nest up to three levels. */
export interface Collection {
  id: string;
  name: string;
  parent_id: string | null;
  /** Queries directly in this folder. */
  query_count: number;
  created_at: string;
  updated_at: string;
}

export interface SavedQuery {
  id: string;
  workspace_id: string;
  /** Creator; null once their account is deleted. */
  created_by: string | null;
  name: string;
  description: string | null;
  sql_content: string;
  connection_id: string | null;
  collection_id: string | null;
  created_at: string;
  updated_at: string;
  pinned: boolean;
  tags: string[];
}

export interface SavedQueryInput {
  name: string;
  description?: string | null;
  sql_content: string;
  connection_id?: string | null;
  collection_id?: string | null;
  tags?: string[];
}

export interface TagCount {
  tag: string;
  count: number;
}

export interface WorkspaceTab {
  id: string;
  title: string;
  sql: string;
  saved_query_id: string | null;
  saved_sql: string | null;
}

export interface WorkspaceSession {
  tabs: WorkspaceTab[];
  active_tab_id: string | null;
  active_connection_id: string | null;
}

export interface SessionResponse {
  session: WorkspaceSession | null;
  updated_at: string | null;
}

export interface WorkspaceSettings {
  history_retention_days: number;
}

export interface Stats {
  days: number;
  totals: {
    executions: number;
    successful: number;
    failed: number;
    avg_execution_ms: number | null;
    p95_execution_ms: number | null;
    saved_queries: number;
    connections: number;
  };
  daily: { day: string; successful: number; failed: number; avg_execution_ms: number | null }[];
  by_connection: { connection_id: string | null; connection_name: string | null; executions: number }[];
  slowest: HistoryEntry[];
}
