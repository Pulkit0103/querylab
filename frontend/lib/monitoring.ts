/** Pure helpers for the monitoring views. */
import type { BlockingNode, Privileges, Session } from "@/types/monitoring";

/** Auto-refresh choices; the server allows a refresh every 2 s on average. */
export const REFRESH_OPTIONS = [
  { value: 5000, label: "5 s" },
  { value: 10000, label: "10 s" },
  { value: 30000, label: "30 s" },
  { value: 0, label: "Off" },
] as const;

/** Open transactions older than this are flagged. */
export const LONG_TRANSACTION_MS = 60_000;

/** Why some data is hidden, in plain words; null when nothing is. */
export function privilegeNotice(p: Privileges): string | null {
  if (p.superuser || p.read_all_stats) return null;
  return (
    `Connected as ${p.role}, which cannot see other roles' statements: they show as hidden. ` +
    "Grant the role pg_monitor (or pg_read_all_stats) to see them."
  );
}

/** Why cancel/terminate is unavailable for a session, or null if allowed. */
export function signalBlocker(p: Privileges, s: Session): string | null {
  if (s.is_self) return "This is QueryLab's monitoring session.";
  if (p.read_only_connection) return "Enable read-write mode in the connection settings to cancel or terminate sessions.";
  if (!p.superuser && !p.signal_backend && s.user !== null && s.user !== p.role)
    return `Only ${s.user}, a superuser or a member of pg_signal_backend can signal this session.`;
  return null;
}

export type SessionTone = "active" | "waiting" | "idle" | "idle-in-transaction" | "other";

/** Display state: waiting sessions and long idle transactions stand out. */
export function sessionTone(s: Session): SessionTone {
  if (s.blocked_by.length > 0 || s.wait_event_type === "Lock") return "waiting";
  if (s.state === "active") return "active";
  if (s.state?.startsWith("idle in transaction")) return "idle-in-transaction";
  if (s.state === "idle") return "idle";
  return "other";
}

export interface SessionFilter {
  search: string;
  /** Hide idle client sessions. */
  activeOnly: boolean;
  /** Hide background workers and QueryLab's own session. */
  clientsOnly: boolean;
}

export const defaultSessionFilter: SessionFilter = { search: "", activeOnly: false, clientsOnly: true };

export function filterSessions(sessions: Session[], f: SessionFilter): Session[] {
  const q = f.search.trim().toLowerCase();
  return sessions.filter((s) => {
    if (f.clientsOnly && (s.is_self || (s.backend_type !== null && s.backend_type !== "client backend"))) return false;
    if (f.activeOnly && sessionTone(s) === "idle") return false;
    if (!q) return true;
    return [String(s.pid), s.user, s.application, s.client, s.query, s.state]
      .filter((v): v is string => typeof v === "string")
      .some((v) => v.toLowerCase().includes(q));
  });
}

/** Depth-first rows for rendering the blocking tree. */
export function flattenBlocking(tree: BlockingNode[], depth = 0): Array<{ node: BlockingNode; depth: number }> {
  return tree.flatMap((node) => [{ node, depth }, ...flattenBlocking(node.children, depth + 1)]);
}

/** `part` as a share of `total` (0-1), safe for zero totals. */
export const share = (part: number, total: number) => (total > 0 ? Math.min(1, Math.max(0, part / total)) : 0);

/** Dead tuples as a share of all tuples, when statistics exist. */
export function deadRatio(live: number | null, dead: number | null): number | null {
  if (live === null || dead === null || live + dead === 0) return null;
  return dead / (live + dead);
}
