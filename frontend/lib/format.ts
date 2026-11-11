import { format as formatSqlText } from "sql-formatter";

const numberFormat = new Intl.NumberFormat("en-US");

export function formatNumber(n: number | null | undefined): string {
  if (n === null || n === undefined || Number.isNaN(n)) return "—";
  return numberFormat.format(n);
}

/** Human-friendly duration: `850 ms`, `1.24 s`, `2m 05s`. */
export function formatDuration(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return "—";
  if (ms < 1) return "<1 ms";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(2)} s`;
  const minutes = Math.floor(ms / 60_000);
  const seconds = Math.round((ms % 60_000) / 1000);
  return `${minutes}m ${seconds.toString().padStart(2, "0")}s`;
}

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 && unit > 0 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

const relative = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

export function formatRelativeTime(iso: string, now: Date = new Date()): string {
  const diffSec = Math.round((new Date(iso).getTime() - now.getTime()) / 1000);
  const abs = Math.abs(diffSec);
  if (abs < 60) return relative.format(diffSec, "second");
  if (abs < 3600) return relative.format(Math.round(diffSec / 60), "minute");
  if (abs < 86_400) return relative.format(Math.round(diffSec / 3600), "hour");
  if (abs < 2_592_000) return relative.format(Math.round(diffSec / 86_400), "day");
  return new Date(iso).toLocaleDateString();
}

/** Formats SQL using PostgreSQL dialect rules. Throws on unparseable input. */
export function formatSql(sql: string): string {
  return formatSqlText(sql, {
    language: "postgresql",
    keywordCase: "upper",
    tabWidth: 2,
    linesBetweenQueries: 2,
  });
}

/** Renders a cell value as display text. */
export function cellToText(value: unknown): string {
  if (value === null || value === undefined) return "NULL";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}
