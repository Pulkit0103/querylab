import { describe, expect, it } from "vitest";

import {
  emptyHistoryFilters,
  flattenTree,
  folderPath,
  folderTree,
  fromSession,
  historyQuery,
  isFiltered,
  moveTargets,
  normalizeTags,
  restoreAction,
  tagError,
  toSession,
} from "@/lib/library";
import type { Collection, WorkspaceSession } from "@/types/library";

const folder = (id: string, name: string, parent_id: string | null = null): Collection => ({
  id,
  name,
  parent_id,
  query_count: 0,
  created_at: "",
  updated_at: "",
});

// a > b > c, a > d, x
const folders = [folder("c", "Charlie", "b"), folder("x", "Xray"), folder("b", "bravo", "a"), folder("d", "Delta", "a"), folder("a", "Alpha")];

describe("folder tree", () => {
  it("nests, sorts by name and records depth", () => {
    const flat = flattenTree(folderTree(folders)).map((n) => `${n.depth}:${n.folder.name}`);
    expect(flat).toEqual(["1:Alpha", "2:bravo", "3:Charlie", "2:Delta", "1:Xray"]);
    expect(folderPath(folders, "c")).toBe("Alpha / bravo / Charlie");
    expect(folderPath(folders, "missing")).toBe("");
  });

  it("shows orphans at the top level and survives cycles in bad data", () => {
    const odd = [folder("o", "Orphan", "gone"), folder("p", "P", "q"), folder("q", "Q", "p")];
    expect(flattenTree(folderTree(odd)).map((n) => n.folder.name)).toEqual(["Orphan"]);
    expect(folderPath(odd, "p")).toBe("Q / P");
  });

  it("only offers move targets that keep the tree valid", () => {
    // b has a child, so it can only go under top-level folders (depth 1 + height 2 = 3).
    const targets = (id: string) => moveTargets(folders, id).map((n) => n.folder.id);
    expect(targets("b")).toEqual(["a", "x"]);
    // a's subtree is a, b, c, d (height 3): nowhere but the top level.
    expect(targets("a")).toEqual([]);
    // A leaf can go anywhere except under depth-3 folders or itself.
    expect(targets("d")).toEqual(["a", "b", "x"]);
  });
});

describe("tags", () => {
  it("normalizes like the server", () => {
    expect(normalizeTags([" Ops", "ops", "", "Daily "])).toEqual(["ops", "daily"]);
  });

  it("explains why a tag cannot be added", () => {
    expect(tagError([], "  ")).toBe("Enter a tag");
    expect(tagError([], "a,b")).toMatch(/commas/);
    expect(tagError([], "x".repeat(41))).toMatch(/40/);
    expect(tagError(["ops"], "OPS")).toBe("Already added");
    expect(tagError(Array.from({ length: 20 }, (_, i) => `t${i}`), "new")).toMatch(/20/);
    expect(tagError(["ops"], "daily")).toBeNull();
  });
});

describe("history filters", () => {
  it("maps controls to query parameters", () => {
    expect(historyQuery(emptyHistoryFilters)).toEqual({});
    expect(isFiltered(emptyHistoryFilters)).toBe(false);
    const q = historyQuery({
      ...emptyHistoryFilters,
      search: "  orders ",
      status: "error",
      kind: "data_apply",
      duration: "medium",
      fromDate: "2026-11-01",
      toDate: "2026-11-03",
      pinnedOnly: true,
    });
    expect(q).toMatchObject({ q: "orders", status: "error", kind: "data_apply", min_ms: 100, max_ms: 999, pinned: true });
    // Local midnight of the first day up to (excluding) the day after the last.
    expect(new Date(q.from!).getTime()).toBe(new Date(2026, 10, 1).getTime());
    expect(new Date(q.to!).getTime()).toBe(new Date(2026, 10, 4).getTime());
    expect(historyQuery({ ...emptyHistoryFilters, duration: "very-slow" })).toEqual({ min_ms: 10000 });
    expect(historyQuery({ ...emptyHistoryFilters, fromDate: "not-a-date" })).toEqual({});
  });
});

describe("tab session", () => {
  const session: WorkspaceSession = {
    tabs: [{ id: "t1", title: "A", sql: "select 1", saved_query_id: "q1", saved_sql: "select 1" }],
    active_tab_id: "t1",
    active_connection_id: "c1",
  };

  it("round-trips between local tabs and the API shape", () => {
    const local = fromSession(session);
    expect(local.tabs[0]).toEqual({ id: "t1", title: "A", sql: "select 1", savedQueryId: "q1", savedSql: "select 1" });
    expect(toSession(local.tabs, local.activeTabId, "c1")).toEqual(session);
    expect(toSession(local.tabs, "gone", null).active_tab_id).toBeNull();
  });

  it("restores the newer side and never uploads another user's tabs", () => {
    const server = { session, updated_at: "2026-11-21T10:00:00Z" };
    const none = { session: null, updated_at: null };
    // First sign-in on this browser: adopt the server tabs.
    expect(restoreAction("u1", { owner: null, syncedAt: null }, server)).toBe("adopt");
    // Server changed elsewhere since the last sync.
    expect(restoreAction("u1", { owner: "u1", syncedAt: "2026-11-21T09:00:00Z" }, server)).toBe("adopt");
    // Local is up to date.
    expect(restoreAction("u1", { owner: "u1", syncedAt: "2026-11-21T10:00:00Z" }, server)).toBe("push");
    // Different user in this browser.
    expect(restoreAction("u2", { owner: "u1", syncedAt: "2026-11-21T11:00:00Z" }, server)).toBe("adopt");
    expect(restoreAction("u2", { owner: "u1", syncedAt: "2026-11-21T11:00:00Z" }, none)).toBe("reset");
    // Nothing on the server yet: upload local tabs.
    expect(restoreAction("u1", { owner: null, syncedAt: null }, none)).toBe("push");
  });
});
