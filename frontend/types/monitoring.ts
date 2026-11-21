/** Live server monitoring (mirrors `models::monitoring`). */

export interface Privileges {
  role: string;
  superuser: boolean;
  /** Sees every session's statement (pg_read_all_stats / pg_monitor). */
  read_all_stats: boolean;
  /** May cancel other roles' sessions (pg_signal_backend). */
  signal_backend: boolean;
  /** QueryLab setting: cancel and terminate need a read-write connection. */
  read_only_connection: boolean;
}

export interface Session {
  pid: number;
  user: string | null;
  application: string | null;
  client: string | null;
  backend_type: string | null;
  state: string | null;
  wait_event_type: string | null;
  wait_event: string | null;
  backend_start: string | null;
  xact_start: string | null;
  query_start: string | null;
  query_ms: number | null;
  xact_ms: number | null;
  query: string | null;
  query_hidden: boolean;
  blocked_by: number[];
  is_self: boolean;
}

export interface ActivityResponse {
  privileges: Privileges;
  sessions: Session[];
  server_time: string;
}

export interface LockInfo {
  pid: number;
  locktype: string;
  mode: string;
  granted: boolean;
  relation: string | null;
}

export interface BlockingNode {
  pid: number;
  blocked_by: number[];
  children: BlockingNode[];
}

export interface LocksResponse {
  privileges: Privileges;
  tree: BlockingNode[];
  sessions: Record<string, Session>;
  locks: LockInfo[];
  waiting: number;
}

export interface SchemaSize {
  schema: string;
  total_bytes: number;
  tables: number;
}

export interface TableSize {
  schema: string;
  name: string;
  kind: "table" | "materialized_view";
  total_bytes: number;
  table_bytes: number;
  index_bytes: number;
  estimated_rows: number | null;
  live_rows: number | null;
  dead_rows: number | null;
  last_vacuum: string | null;
  last_analyze: string | null;
}

export interface IndexSize {
  schema: string;
  table: string;
  name: string;
  bytes: number;
  scans: number | null;
  unique: boolean;
  primary: boolean;
}

export interface SizesResponse {
  database: string;
  database_bytes: number;
  schemas: SchemaSize[];
  tables: TableSize[];
  indexes: IndexSize[];
}

export type TopQueriesSort = "total" | "mean" | "calls";

export interface TopQuery {
  query_id: string | null;
  query: string | null;
  calls: number;
  total_ms: number;
  mean_ms: number;
  max_ms: number;
  rows: number;
  shared_hit: number;
  shared_read: number;
}

export interface TopQueriesResponse {
  available: boolean;
  reason: string | null;
  privileges: Privileges;
  queries: TopQuery[];
}

export type BackendSignal = "cancel" | "terminate";
