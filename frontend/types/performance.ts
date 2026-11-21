/** Performance history and saved plans (mirrors `models::performance`). */
import type { HistoryEntry } from "@/types/library";
import type { ExplainResult } from "@/types/query";

export interface FingerprintStats {
  /** 64-bit, as a string. */
  fingerprint: string;
  sample_sql: string;
  normalized_sql: string;
  connection_id: string | null;
  connection_name: string | null;
  executions: number;
  errors: number;
  avg_ms: number | null;
  p50_ms: number | null;
  p95_ms: number | null;
  max_ms: number | null;
  total_ms: number | null;
  last_executed_at: string;
}

export interface TimelinePoint {
  day: string;
  executions: number;
  errors: number;
  p50_ms: number | null;
  p95_ms: number | null;
  max_ms: number | null;
}

export interface FingerprintTimeline {
  stats: FingerprintStats;
  points: TimelinePoint[];
  recent: HistoryEntry[];
}

export interface SavedPlanSummary {
  id: string;
  name: string;
  connection_id: string | null;
  connection_name: string | null;
  sql_preview: string;
  analyzed: boolean;
  execution_time_ms: number | null;
  total_cost: number | null;
  created_at: string;
}

export interface SavedPlan {
  id: string;
  name: string;
  connection_id: string | null;
  connection_name: string | null;
  sql_content: string;
  plan: ExplainResult;
  analyzed: boolean;
  created_at: string;
}
