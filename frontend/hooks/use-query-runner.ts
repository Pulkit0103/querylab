"use client";

import { useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { api, ApiError } from "@/lib/api-client";
import { formatDuration, formatNumber } from "@/lib/format";
import { useEditorStore } from "@/stores/editor-store";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { ExplainResult, QueryResult, SqlErrorDetails } from "@/types/query";

interface InFlight {
  controller: AbortController;
  executionId: string;
}

/** In-flight requests per editor tab, used for cancellation. */
const inflight = new Map<string, InFlight>();

export interface RunOptions {
  /** Character offset of `sql` within the editor document (for error markers). */
  offset?: number;
}

function begin(tabId: string, kind: "execute" | "explain"): InFlight {
  inflight.get(tabId)?.controller.abort();
  const entry: InFlight = { controller: new AbortController(), executionId: crypto.randomUUID() };
  inflight.set(tabId, entry);
  useEditorStore.getState().startExec(tabId, kind, entry.executionId);
  return entry;
}

function finish(tabId: string, entry: InFlight) {
  if (inflight.get(tabId) === entry) inflight.delete(tabId);
}

function describeError(err: ApiError): { text: string; detail?: string } {
  const d = (err.details ?? {}) as SqlErrorDetails;
  const lines: string[] = [];
  if (d.sqlstate) lines.push(`SQLSTATE ${d.sqlstate}`);
  if (d.detail) lines.push(`Detail: ${d.detail}`);
  if (d.hint) lines.push(`Hint: ${d.hint}`);
  if (typeof d.position === "number") lines.push(`Position: ${d.position}`);
  return { text: err.message, detail: lines.length ? lines.join("\n") : undefined };
}

export function useQueryRunner() {
  const qc = useQueryClient();
  const store = useEditorStore;
  const maxRows = useWorkspaceStore((s) => s.maxRows);

  const run = useCallback(
    async (tabId: string, sql: string, opts: RunOptions = {}) => {
      const connectionId = useWorkspaceStore.getState().activeConnectionId;
      if (!sql.trim()) return;
      if (!connectionId) {
        toast.error("Select or add a database connection first.");
        return;
      }
      const s = store.getState();
      const entry = begin(tabId, "execute");
      try {
        const result = await api.post<QueryResult>(
          "/api/query/execute",
          { connection_id: connectionId, query: sql, max_rows: maxRows, execution_id: entry.executionId },
          { signal: entry.controller.signal },
        );
        s.finishExec(tabId, result);
        const what =
          result.columns.length === 0 && result.rows_affected !== null
            ? `${formatNumber(result.rows_affected)} rows affected`
            : `${formatNumber(result.row_count)} rows`;
        s.addMessage(tabId, {
          level: "success",
          text: `${result.command ?? "Statement"} completed · ${what} · ${formatDuration(result.execution_time_ms)}`,
        });
        if (result.truncated) {
          s.addMessage(tabId, {
            level: "warning",
            text: `Result truncated to ${formatNumber(result.row_count)} rows. Add a LIMIT or raise the row limit in Settings.`,
          });
        }
      } catch (err) {
        if (err instanceof DOMException && err.name === "AbortError") return;
        const apiErr =
          err instanceof ApiError ? err : new ApiError(0, "unknown", err instanceof Error ? err.message : "Unknown error");
        const { text, detail } = describeError(apiErr);
        s.failExec(tabId, { code: apiErr.code, message: text, details: { ...(apiErr.details as object), sqlOffset: opts.offset ?? 0 } });
        s.addMessage(tabId, {
          level: apiErr.code === "query_cancelled" ? "warning" : "error",
          text: apiErr.code === "query_cancelled" ? "Query cancelled." : text,
          detail,
        });
      } finally {
        finish(tabId, entry);
        // Execution may be recorded in history.
        void qc.invalidateQueries({ queryKey: ["history"] });
        void qc.invalidateQueries({ queryKey: ["stats"] });
      }
    },
    [qc, store, maxRows],
  );

  const explain = useCallback(
    async (tabId: string, sql: string, { analyze = false, offset = 0 }: { analyze?: boolean; offset?: number } = {}) => {
      const connectionId = useWorkspaceStore.getState().activeConnectionId;
      if (!sql.trim()) return;
      if (!connectionId) {
        toast.error("Select or add a database connection first.");
        return;
      }
      const s = store.getState();
      const entry = begin(tabId, "explain");
      try {
        const plan = await api.post<ExplainResult>(
          "/api/query/explain",
          {
            connection_id: connectionId,
            query: sql,
            analyze,
            confirm_analyze: analyze,
            execution_id: entry.executionId,
          },
          { signal: entry.controller.signal },
        );
        s.finishExplain(tabId, plan);
        s.addMessage(tabId, {
          level: "info",
          text: analyze
            ? `EXPLAIN ANALYZE completed · planning ${formatDuration(plan.planning_time_ms)} · execution ${formatDuration(plan.execution_time_ms)}`
            : "Execution plan generated.",
        });
      } catch (err) {
        if (err instanceof DOMException && err.name === "AbortError") return;
        const apiErr =
          err instanceof ApiError ? err : new ApiError(0, "unknown", err instanceof Error ? err.message : "Unknown error");
        const { text, detail } = describeError(apiErr);
        s.failExplain(tabId, { code: apiErr.code, message: text, details: { ...(apiErr.details as object), sqlOffset: offset } });
        s.addMessage(tabId, { level: "error", text: `EXPLAIN failed: ${text}`, detail });
      } finally {
        finish(tabId, entry);
      }
    },
    [store],
  );

  /** Cancels server-side first (pg_cancel_backend), then aborts the request. */
  const cancel = useCallback(
    async (tabId: string) => {
      const entry = inflight.get(tabId);
      if (!entry) return;
      try {
        await api.post(`/api/query/${entry.executionId}/cancel`);
      } catch {
        // The execution may have just finished; fall through to abort.
      }
      // Give the server a moment to report the cancellation, then abort.
      setTimeout(() => {
        if (inflight.get(tabId) === entry) {
          entry.controller.abort();
          inflight.delete(tabId);
          store.getState().failExec(tabId, { code: "query_cancelled", message: "Query cancelled." });
          store.getState().addMessage(tabId, { level: "warning", text: "Query cancelled." });
        }
      }, 1500);
    },
    [store],
  );

  return { run, explain, cancel };
}
