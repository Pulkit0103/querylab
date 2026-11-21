import { describe, expect, it } from "vitest";

import { defaultPlanName } from "@/components/results/save-plan-button";
import {
  deadRatio,
  defaultSessionFilter,
  filterSessions,
  flattenBlocking,
  privilegeNotice,
  sessionTone,
  share,
  signalBlocker,
} from "@/lib/monitoring";
import { analyzePlan } from "@/lib/plan";
import { comparePlans, describeRatio } from "@/lib/plan-compare";
import type { Privileges, Session } from "@/types/monitoring";
import type { PlanNode } from "@/types/query";

const privileges = (p: Partial<Privileges> = {}): Privileges => ({
  role: "reader",
  superuser: false,
  read_all_stats: false,
  signal_backend: false,
  read_only_connection: false,
  ...p,
});

const session = (s: Partial<Session> = {}): Session => ({
  pid: 100,
  user: "reader",
  application: "psql",
  client: "10.0.0.5",
  backend_type: "client backend",
  state: "active",
  wait_event_type: null,
  wait_event: null,
  backend_start: null,
  xact_start: null,
  query_start: null,
  query_ms: 10,
  xact_ms: 10,
  query: "select 1",
  query_hidden: false,
  blocked_by: [],
  is_self: false,
  ...s,
});

describe("privileges", () => {
  it("explains hidden statements only when they are hidden", () => {
    expect(privilegeNotice(privileges())).toMatch(/pg_monitor/);
    expect(privilegeNotice(privileges({ read_all_stats: true }))).toBeNull();
    expect(privilegeNotice(privileges({ superuser: true }))).toBeNull();
  });

  it("says why a session cannot be signalled", () => {
    expect(signalBlocker(privileges(), session({ is_self: true }))).toMatch(/monitoring session/);
    expect(signalBlocker(privileges({ read_only_connection: true }), session())).toMatch(/read-write/);
    expect(signalBlocker(privileges(), session({ user: "owner" }))).toMatch(/pg_signal_backend/);
    expect(signalBlocker(privileges({ signal_backend: true }), session({ user: "owner" }))).toBeNull();
    expect(signalBlocker(privileges(), session())).toBeNull();
  });
});

describe("sessions", () => {
  it("classifies states", () => {
    expect(sessionTone(session())).toBe("active");
    expect(sessionTone(session({ blocked_by: [7] }))).toBe("waiting");
    expect(sessionTone(session({ state: "idle in transaction (aborted)" }))).toBe("idle-in-transaction");
    expect(sessionTone(session({ state: "idle" }))).toBe("idle");
    expect(sessionTone(session({ state: null }))).toBe("other");
  });

  it("filters by text, idleness and backend type", () => {
    const all = [
      session({ pid: 1 }),
      session({ pid: 2, state: "idle", query: "select now()" }),
      session({ pid: 3, backend_type: "autovacuum worker", user: null }),
      session({ pid: 4, is_self: true }),
    ];
    const pids = (f = defaultSessionFilter) => filterSessions(all, f).map((s) => s.pid);
    expect(pids()).toEqual([1, 2]);
    expect(pids({ ...defaultSessionFilter, activeOnly: true })).toEqual([1]);
    expect(pids({ ...defaultSessionFilter, clientsOnly: false })).toEqual([1, 2, 3, 4]);
    expect(pids({ ...defaultSessionFilter, search: "NOW" })).toEqual([2]);
    expect(pids({ ...defaultSessionFilter, search: "2" })).toEqual([2]);
  });

  it("flattens the blocking tree depth-first", () => {
    const rows = flattenBlocking([
      { pid: 1, blocked_by: [], children: [{ pid: 2, blocked_by: [1], children: [{ pid: 3, blocked_by: [2], children: [] }] }] },
      { pid: 9, blocked_by: [], children: [] },
    ]);
    expect(rows.map((r) => `${r.depth}:${r.node.pid}`)).toEqual(["0:1", "1:2", "2:3", "0:9"]);
  });

  it("computes shares safely", () => {
    expect(share(5, 10)).toBe(0.5);
    expect(share(5, 0)).toBe(0);
    expect(share(20, 10)).toBe(1);
    expect(deadRatio(90, 10)).toBe(0.1);
    expect(deadRatio(null, 10)).toBeNull();
    expect(deadRatio(0, 0)).toBeNull();
  });
});

const scan = (type: string, relation: string, extra: Partial<PlanNode> = {}): PlanNode => ({
  "Node Type": type,
  "Relation Name": relation,
  Schema: "sales",
  Alias: relation,
  "Total Cost": 100,
  "Plan Rows": 10,
  ...extra,
});

describe("plan comparison", () => {
  const before = analyzePlan(
    {
      "Node Type": "Hash Join",
      "Total Cost": 500,
      "Plan Rows": 10,
      "Actual Total Time": 40,
      "Actual Rows": 10,
      "Actual Loops": 1,
      Plans: [scan("Seq Scan", "orders", { "Total Cost": 400 }), { "Node Type": "Hash", "Total Cost": 50, Plans: [scan("Seq Scan", "customers")] }],
    } as PlanNode,
    true,
  );
  const after = analyzePlan(
    {
      "Node Type": "Nested Loop",
      "Total Cost": 50,
      "Plan Rows": 10,
      "Actual Total Time": 4,
      "Actual Rows": 10,
      "Actual Loops": 1,
      Plans: [
        scan("Index Scan", "orders", { "Index Name": "orders_customer_idx", "Total Cost": 20 }),
        scan("Seq Scan", "customers"),
      ],
    } as PlanNode,
    true,
  );

  it("reports metrics, access changes and operation counts", () => {
    const c = comparePlans(before, after);
    const time = c.metrics.find((m) => m.label === "Execution time")!;
    expect(time.before).toBe(40);
    expect(time.after).toBe(4);
    expect(describeRatio(time.ratio, "ms")).toBe("10× faster");
    const cost = c.metrics.find((m) => m.label === "Total cost")!;
    expect(describeRatio(cost.ratio, "cost")).toBe("10× cheaper");
    const orders = c.access.find((a) => a.target.includes("orders"))!;
    expect(orders.changed).toBe(true);
    expect(orders.before).toEqual(["Seq Scan"]);
    expect(orders.after).toEqual(["Index Scan using orders_customer_idx"]);
    expect(c.access.find((a) => a.target.includes("customers"))!.changed).toBe(false);
    expect(c.operations.map((o) => o.title)).toEqual(["Hash", "Hash Join", "Index Scan", "Nested Loop", "Seq Scan"]);
  });

  it("describes ratios", () => {
    expect(describeRatio(1.5, "ms")).toBe("1.5× slower");
    expect(describeRatio(1, "cost")).toBe("same");
    expect(describeRatio(null, "ms")).toBeNull();
    expect(describeRatio(0.5, "count")).toBe("fewer");
  });

  it("names saved plans after their statement", () => {
    expect(defaultPlanName("-- note\nSELECT *\n  FROM orders", false)).toBe("SELECT * FROM orders");
    expect(defaultPlanName("select " + "x".repeat(100), true)).toMatch(/^select x+… \(analyze\)$/);
    expect(defaultPlanName("  ", false)).toBe("Plan");
  });
});
