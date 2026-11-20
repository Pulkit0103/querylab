/**
 * Pure helpers for the query library: folder trees, tags, history filters
 * and the synced tab session.
 */
import type { Collection, HistoryFilters, HistoryKind, HistoryStatus, WorkspaceSession } from "@/types/library";

/** Folders nest at most this deep (mirrors the server). */
export const MAX_FOLDER_DEPTH = 3;
export const MAX_TAGS = 20;
export const MAX_TAG_CHARS = 40;

// ---------------------------------------------------------------------------
// Folders
// ---------------------------------------------------------------------------

export interface FolderNode {
  folder: Collection;
  depth: number;
  children: FolderNode[];
}

const byName = (a: Collection, b: Collection) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" });

/** Builds the folder tree. Orphans (parent missing) are shown at the top level. */
export function folderTree(folders: Collection[]): FolderNode[] {
  const ids = new Set(folders.map((f) => f.id));
  const build = (parent: string | null, depth: number, seen: Set<string>): FolderNode[] =>
    folders
      .filter((f) => (parent === null ? f.parent_id === null || !ids.has(f.parent_id) : f.parent_id === parent))
      .filter((f) => !seen.has(f.id))
      .sort(byName)
      .map((folder) => {
        const next = new Set(seen).add(folder.id);
        return { folder, depth, children: build(folder.id, depth + 1, next) };
      });
  return build(null, 1, new Set());
}

/** Depth-first list of folders, for selects and the sidebar. */
export function flattenTree(nodes: FolderNode[]): FolderNode[] {
  return nodes.flatMap((n) => [n, ...flattenTree(n.children)]);
}

/** `Parent / Child` path of a folder. */
export function folderPath(folders: Collection[], id: string): string {
  const byId = new Map(folders.map((f) => [f.id, f]));
  const names: string[] = [];
  let current = byId.get(id);
  const seen = new Set<string>();
  while (current && !seen.has(current.id)) {
    seen.add(current.id);
    names.unshift(current.name);
    current = current.parent_id ? byId.get(current.parent_id) : undefined;
  }
  return names.join(" / ");
}

function subtree(folders: Collection[], id: string): { ids: Set<string>; height: number } {
  const ids = new Set<string>();
  const walk = (current: string, level: number): number => {
    if (ids.has(current)) return level - 1;
    ids.add(current);
    const heights = folders.filter((f) => f.parent_id === current).map((f) => walk(f.id, level + 1));
    return Math.max(level, ...heights);
  };
  const height = walk(id, 1);
  return { ids, height };
}

/**
 * Folders that `id` can be moved under without creating a cycle or
 * exceeding the depth limit. The server enforces the same rules.
 */
export function moveTargets(folders: Collection[], id: string): FolderNode[] {
  const { ids, height } = subtree(folders, id);
  return flattenTree(folderTree(folders)).filter((n) => !ids.has(n.folder.id) && n.depth + height <= MAX_FOLDER_DEPTH);
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

/** Lower-cased, trimmed, de-duplicated (mirrors the server). */
export function normalizeTags(tags: string[]): string[] {
  const out: string[] = [];
  for (const raw of tags) {
    const tag = raw.trim().toLowerCase();
    if (tag && !out.includes(tag)) out.push(tag);
  }
  return out;
}

/** Why a tag cannot be added, or null if it can. */
export function tagError(tags: string[], candidate: string): string | null {
  const tag = candidate.trim().toLowerCase();
  if (!tag) return "Enter a tag";
  if (tag.includes(",")) return "Tags cannot contain commas";
  if ([...tag].length > MAX_TAG_CHARS) return `Tags are at most ${MAX_TAG_CHARS} characters`;
  if (tags.includes(tag)) return "Already added";
  if (tags.length >= MAX_TAGS) return `At most ${MAX_TAGS} tags`;
  return null;
}

// ---------------------------------------------------------------------------
// History filters
// ---------------------------------------------------------------------------

export const DURATION_PRESETS = [
  { value: "any", label: "Any duration" },
  { value: "fast", label: "Under 100 ms", max_ms: 99 },
  { value: "medium", label: "100 ms – 1 s", min_ms: 100, max_ms: 999 },
  { value: "slow", label: "1 – 10 s", min_ms: 1000, max_ms: 9999 },
  { value: "very-slow", label: "10 s or more", min_ms: 10000 },
] as const;

export type DurationPreset = (typeof DURATION_PRESETS)[number]["value"];

export interface HistoryFilterState {
  search: string;
  status: HistoryStatus | "all";
  kind: HistoryKind | "all";
  duration: DurationPreset;
  /** `YYYY-MM-DD` in the browser's time zone; both days are included. */
  fromDate: string;
  toDate: string;
  pinnedOnly: boolean;
}

export const emptyHistoryFilters: HistoryFilterState = {
  search: "",
  status: "all",
  kind: "all",
  duration: "any",
  fromDate: "",
  toDate: "",
  pinnedOnly: false,
};

const localMidnight = (date: string, addDays = 0) => {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d + addDays).toISOString();
};

/** API query parameters for the history filter controls. */
export function historyQuery(state: HistoryFilterState): HistoryFilters {
  const out: HistoryFilters = {};
  const q = state.search.trim();
  if (q) out.q = q;
  if (state.status !== "all") out.status = state.status;
  if (state.kind !== "all") out.kind = state.kind;
  const preset = DURATION_PRESETS.find((p) => p.value === state.duration);
  if (preset && "min_ms" in preset) out.min_ms = preset.min_ms;
  if (preset && "max_ms" in preset) out.max_ms = preset.max_ms;
  if (/^\d{4}-\d{2}-\d{2}$/.test(state.fromDate)) out.from = localMidnight(state.fromDate);
  if (/^\d{4}-\d{2}-\d{2}$/.test(state.toDate)) out.to = localMidnight(state.toDate, 1);
  if (state.pinnedOnly) out.pinned = true;
  return out;
}

export const isFiltered = (state: HistoryFilterState) =>
  JSON.stringify(state) !== JSON.stringify(emptyHistoryFilters);

// ---------------------------------------------------------------------------
// Tab session
// ---------------------------------------------------------------------------

interface LocalTab {
  id: string;
  title: string;
  sql: string;
  savedQueryId: string | null;
  savedSql: string | null;
}

export function toSession(tabs: LocalTab[], activeTabId: string, activeConnectionId: string | null): WorkspaceSession {
  return {
    tabs: tabs.map((t) => ({
      id: t.id,
      title: t.title,
      sql: t.sql,
      saved_query_id: t.savedQueryId,
      saved_sql: t.savedSql,
    })),
    active_tab_id: tabs.some((t) => t.id === activeTabId) ? activeTabId : null,
    active_connection_id: activeConnectionId,
  };
}

export type RestoreAction =
  /** Replace local tabs with the server session. */
  | "adopt"
  /** Local tabs belong to someone else and there is no server session: start fresh. */
  | "reset"
  /** Local tabs are current: upload them. */
  | "push";

/**
 * What to do with local tabs after sign-in. Tabs cached for a different user
 * in this browser are never uploaded to the current account.
 */
export function restoreAction(
  userId: string,
  local: { owner: string | null; syncedAt: string | null },
  server: { session: WorkspaceSession | null; updated_at: string | null },
): RestoreAction {
  const sameOwner = local.owner === userId;
  if (server.session && server.updated_at) {
    if (!sameOwner || !local.syncedAt) return "adopt";
    return Date.parse(server.updated_at) > Date.parse(local.syncedAt) ? "adopt" : "push";
  }
  return local.owner !== null && !sameOwner ? "reset" : "push";
}

export function fromSession(session: WorkspaceSession): { tabs: LocalTab[]; activeTabId: string } {
  const tabs = session.tabs.map((t) => ({
    id: t.id,
    title: t.title,
    sql: t.sql,
    savedQueryId: t.saved_query_id,
    savedSql: t.saved_sql,
  }));
  return { tabs, activeTabId: session.active_tab_id ?? tabs[0]?.id ?? "" };
}
