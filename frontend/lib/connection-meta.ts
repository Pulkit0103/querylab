/** Presentation helpers for connections: environments, statuses, grouping. */

import type { Connection, ConnectionGroup, ConnectionStatus, Environment } from "@/types/api";

export const ENVIRONMENTS: { value: Environment; label: string; short: string }[] = [
  { value: "development", label: "Development", short: "dev" },
  { value: "staging", label: "Staging", short: "staging" },
  { value: "production", label: "Production", short: "prod" },
];

export function environmentLabel(env: Environment): string {
  return ENVIRONMENTS.find((e) => e.value === env)?.label ?? env;
}

/** Tailwind classes for the environment badge (text + border + tint). */
export const ENVIRONMENT_BADGE: Record<Environment, string> = {
  development: "border-border text-muted-foreground",
  staging: "border-warning/40 bg-warning/10 text-warning",
  production: "border-destructive/40 bg-destructive/10 text-destructive",
};

export const STATUS_LABEL: Record<ConnectionStatus, string> = {
  online: "Online",
  unreachable: "Unreachable",
  auth_failed: "Authentication failed",
  tls_error: "TLS error",
  error: "Connection error",
};

/** Dot colour for a status; untested connections are neutral. */
export function statusTone(status: ConnectionStatus | null): "success" | "destructive" | "warning" | "muted" {
  if (status === null) return "muted";
  if (status === "online") return "success";
  if (status === "auth_failed" || status === "tls_error") return "warning";
  return "destructive";
}

/** Label colours offered in the connection dialog. */
export const CONNECTION_COLORS = [
  "#3b82f6",
  "#10b981",
  "#f59e0b",
  "#ef4444",
  "#8b5cf6",
  "#ec4899",
  "#14b8a6",
  "#64748b",
] as const;

export interface ConnectionSection {
  /** `null` for ungrouped connections. */
  group: ConnectionGroup | null;
  connections: Connection[];
}

/**
 * Splits connections into sections by group (in group order), followed by
 * an "Ungrouped" section. Empty groups are kept so they remain drop
 * targets; the ungrouped section is omitted when empty and groups exist.
 */
export function groupConnections(connections: Connection[], groups: ConnectionGroup[]): ConnectionSection[] {
  const byGroup = new Map<string, Connection[]>();
  const ungrouped: Connection[] = [];
  const known = new Set(groups.map((g) => g.id));
  for (const c of connections) {
    if (c.group_id && known.has(c.group_id)) {
      const list = byGroup.get(c.group_id) ?? [];
      list.push(c);
      byGroup.set(c.group_id, list);
    } else {
      ungrouped.push(c);
    }
  }
  const ordered = [...groups].sort((a, b) => a.position - b.position || a.name.localeCompare(b.name));
  const sections: ConnectionSection[] = ordered.map((g) => ({ group: g, connections: byGroup.get(g.id) ?? [] }));
  if (ungrouped.length > 0 || sections.length === 0) sections.push({ group: null, connections: ungrouped });
  return sections;
}

/** Case-insensitive match on name, host, database and user. */
export function matchesConnection(c: Connection, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [c.name, c.host, c.database, c.username].some((v) => v.toLowerCase().includes(q));
}

/** Cheap structural check mirroring the server's PEM validation. */
export function looksLikePemBundle(pem: string): boolean {
  const text = pem.trim();
  const begins = text.split("-----BEGIN CERTIFICATE-----").length - 1;
  const ends = text.split("-----END CERTIFICATE-----").length - 1;
  return (
    begins >= 1 &&
    begins === ends &&
    text.startsWith("-----BEGIN CERTIFICATE-----") &&
    text.endsWith("-----END CERTIFICATE-----") &&
    text.length <= 64 * 1024
  );
}
