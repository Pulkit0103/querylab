"use client";

import { useCallback } from "react";

import { mockExecute } from "@/lib/mock-data";
import { formatDuration, formatNumber } from "@/lib/format";
import { useEditorStore } from "@/stores/editor-store";

/** In-flight requests per editor tab, used for client-side cancellation. */
const controllers = new Map<string, AbortController>();

export function useQueryRunner() {
  const startExec = useEditorStore((s) => s.startExec);
  const finishExec = useEditorStore((s) => s.finishExec);
  const failExec = useEditorStore((s) => s.failExec);
  const addMessage = useEditorStore((s) => s.addMessage);

  const run = useCallback(
    async (tabId: string, sql: string) => {
      if (!sql.trim()) return;
      controllers.get(tabId)?.abort();
      const controller = new AbortController();
      controllers.set(tabId, controller);
      startExec(tabId, "execute", crypto.randomUUID());
      try {
        const result = await mockExecute(sql, controller.signal);
        finishExec(tabId, result);
        addMessage(tabId, {
          level: "success",
          text: `${result.command ?? "Statement"} returned ${formatNumber(result.row_count)} rows in ${formatDuration(result.execution_time_ms)}.`,
        });
      } catch (err) {
        if (err instanceof DOMException && err.name === "AbortError") {
          failExec(tabId, { code: "cancelled", message: "Query cancelled." });
          addMessage(tabId, { level: "warning", text: "Query cancelled." });
          return;
        }
        const message = err instanceof Error ? err.message : "Unknown error";
        failExec(tabId, { code: "sql_error", message });
        addMessage(tabId, { level: "error", text: message });
      } finally {
        if (controllers.get(tabId) === controller) controllers.delete(tabId);
      }
    },
    [startExec, finishExec, failExec, addMessage],
  );

  const cancel = useCallback((tabId: string) => {
    controllers.get(tabId)?.abort();
  }, []);

  return { run, cancel };
}
