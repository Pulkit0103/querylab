/** History, saved queries, collections and stats (mirrors `models::library`). */

export type HistoryStatus = "success" | "error" | "cancelled" | "timeout";

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
}

export interface HistoryPage {
  items: HistoryEntry[];
  next_cursor: string | null;
}

export interface Collection {
  id: string;
  name: string;
  query_count: number;
  created_at: string;
  updated_at: string;
}

export interface SavedQuery {
  id: string;
  name: string;
  description: string | null;
  sql_content: string;
  connection_id: string | null;
  collection_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface SavedQueryInput {
  name: string;
  description?: string | null;
  sql_content: string;
  connection_id?: string | null;
  collection_id?: string | null;
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
