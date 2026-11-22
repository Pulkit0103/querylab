/** Labels and helpers for the audit log. */
import type { AuditEvent, AuditFilter } from "@/types/account";

export const ACTION_GROUPS: { value: string; label: string }[] = [
  { value: "", label: "All actions" },
  { value: "auth.", label: "Sign-in and account" },
  { value: "query.", label: "Queries" },
  { value: "data.", label: "Data changes and exports" },
  { value: "import.", label: "Imports" },
  { value: "mock_data.", label: "Mock data" },
  { value: "migration.", label: "Migrations" },
  { value: "connection.", label: "Connections" },
  { value: "session.", label: "Database sessions" },
];

const LABELS: Record<string, string> = {
  "auth.registered": "Account created",
  "auth.login": "Signed in",
  "auth.login_failed": "Sign-in failed",
  "auth.logout": "Signed out",
  "auth.password_changed": "Password changed",
  "auth.password_change_failed": "Password change failed",
  "auth.password_reset_requested": "Password reset requested",
  "auth.password_reset": "Password reset",
  "auth.2fa_enabled": "Two-factor turned on",
  "auth.2fa_disabled": "Two-factor turned off",
  "auth.recovery_code_used": "Recovery code used",
  "auth.recovery_codes_regenerated": "Recovery codes regenerated",
  "auth.session_revoked": "Session signed out",
  "auth.sessions_revoked": "Other sessions signed out",
  "query.executed": "Query executed",
  "query.explain_analyze": "EXPLAIN ANALYZE",
  "data.applied": "Data Studio changes applied",
  "data.exported": "Data exported",
  "import.run": "Import",
  "mock_data.run": "Mock data generated",
  "migration.run": "Migration run",
  "connection.created": "Connection created",
  "connection.updated": "Connection updated",
  "connection.deleted": "Connection deleted",
  "session.signalled": "Database session cancelled/terminated",
};

export const actionLabel = (action: string) => LABELS[action] ?? action;

/** Failed or rejected actions, for highlighting. */
export function isFailure(e: Pick<AuditEvent, "action" | "detail">): boolean {
  if (e.action.endsWith("_failed")) return true;
  const status = e.detail?.status;
  return typeof status === "string" && status !== "success" && status !== "completed";
}

/** One-line summary of an event's detail. */
export function summarize(e: Pick<AuditEvent, "detail">): string {
  const d = e.detail;
  if (!d) return "";
  const parts: string[] = [];
  if (typeof d.sql === "string") parts.push(d.sql.replace(/\s+/g, " ").slice(0, 160));
  for (const key of ["status", "rows", "target", "reason", "email", "name"]) {
    const v = d[key];
    if (v !== undefined && v !== null && key !== "sql") parts.push(`${key}: ${String(v)}`);
  }
  return parts.join(" · ");
}

/** Query string for the CSV export of the current filter. */
export function exportQuery(filter: AuditFilter): string {
  const params = new URLSearchParams();
  for (const [k, v] of Object.entries(filter)) if (v) params.set(k, v);
  const qs = params.toString();
  return qs ? `?${qs}` : "";
}
