/**
 * Phase 2 demo execution. Lets the results UI be exercised before the
 * execution engine is wired to the API. Removed once the real endpoint lands.
 */
import type { QueryResult } from "@/types/query";

const names = ["Ada Lovelace", "Grace Hopper", "Linus Torvalds", "Margaret Hamilton", "Ken Thompson", "Barbara Liskov"];

export async function mockExecute(sql: string, signal?: AbortSignal): Promise<QueryResult> {
  const started = performance.now();
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 250 + Math.random() * 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new DOMException("Aborted", "AbortError"));
    });
  });
  if (/\berror\b/i.test(sql)) {
    throw Object.assign(new Error('syntax error at or near "error"'), { code: "sql_error" });
  }
  const rows = Array.from({ length: 137 }, (_, i) => [
    i + 1,
    names[i % names.length],
    `employee${i + 1}@example.com`,
    Math.round(55000 + ((i * 7919) % 95000)),
    i % 7 !== 0,
    new Date(Date.UTC(2020, i % 12, 1 + (i % 28))).toISOString(),
  ]);
  return {
    status: "success",
    columns: [
      { name: "id", type: "int4" },
      { name: "name", type: "text" },
      { name: "email", type: "text" },
      { name: "salary", type: "numeric" },
      { name: "is_active", type: "bool" },
      { name: "hired_at", type: "timestamptz" },
    ],
    rows,
    row_count: rows.length,
    rows_affected: null,
    truncated: false,
    execution_time_ms: Math.round(performance.now() - started),
    command: "SELECT",
  };
}
