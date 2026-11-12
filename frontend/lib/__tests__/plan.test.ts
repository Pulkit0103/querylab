import { describe, expect, it } from "vitest";

import { analyzePlan, categoryOf, formatFactor, misestimateOf, nodeTitle } from "@/lib/plan";
import type { PlanNode } from "@/types/query";

/** EXPLAIN ANALYZE output shape for a hash join over two scans. */
const analyzed: PlanNode = {
  "Node Type": "Hash Join",
  "Join Type": "Left",
  "Total Cost": 100,
  "Plan Rows": 50,
  "Actual Total Time": 10,
  "Actual Rows": 5000,
  "Actual Loops": 1,
  "Hash Cond": "(e.department_id = d.id)",
  Plans: [
    {
      "Node Type": "Seq Scan",
      "Parent Relationship": "Outer",
      "Relation Name": "employees",
      Schema: "public",
      Alias: "e",
      "Total Cost": 60,
      "Plan Rows": 5000,
      "Actual Total Time": 7,
      "Actual Rows": 5000,
      "Actual Loops": 1,
      Filter: "(salary > 10)",
      "Rows Removed by Filter": 95000,
      "Shared Hit Blocks": 12,
      "Shared Read Blocks": 3,
    },
    {
      "Node Type": "Hash",
      "Parent Relationship": "Inner",
      "Total Cost": 20,
      "Plan Rows": 10,
      "Actual Total Time": 2,
      "Actual Rows": 10,
      "Actual Loops": 1,
      "Hash Batches": 4,
      Plans: [
        {
          "Node Type": "Index Scan",
          "Parent Relationship": "Outer",
          "Relation Name": "departments",
          Alias: "departments",
          "Index Name": "departments_pkey",
          "Total Cost": 15,
          "Plan Rows": 10,
          "Actual Total Time": 1.5,
          "Actual Rows": 10,
          "Actual Loops": 1,
        },
      ],
    },
  ],
};

describe("plan analysis", () => {
  it("derives titles like PostgreSQL's text format", () => {
    expect(nodeTitle({ "Node Type": "Hash Join", "Join Type": "Left" })).toBe("Hash Left Join");
    expect(nodeTitle({ "Node Type": "Nested Loop", "Join Type": "Anti" })).toBe("Nested Loop Anti Join");
    expect(nodeTitle({ "Node Type": "Merge Join", "Join Type": "Inner" })).toBe("Merge Join");
    expect(nodeTitle({ "Node Type": "Aggregate", Strategy: "Hashed" })).toBe("HashAggregate");
    expect(nodeTitle({ "Node Type": "Aggregate", Strategy: "Sorted", "Partial Mode": "Finalize" })).toBe(
      "Finalize GroupAggregate",
    );
    expect(nodeTitle({ "Node Type": "Seq Scan", "Parallel Aware": true })).toBe("Parallel Seq Scan");
  });

  it("categorises node types", () => {
    expect(categoryOf("Seq Scan")).toBe("scan");
    expect(categoryOf("Index Only Scan")).toBe("index");
    expect(categoryOf("Bitmap Heap Scan")).toBe("index");
    expect(categoryOf("Nested Loop")).toBe("join");
    expect(categoryOf("Sort")).toBe("sort");
    expect(categoryOf("Aggregate")).toBe("aggregate");
    expect(categoryOf("Limit")).toBe("other");
  });

  it("computes exclusive time, cost and shares", () => {
    const a = analyzePlan(analyzed, true);
    expect(a.nodes.map((n) => n.title)).toEqual(["Hash Left Join", "Seq Scan", "Hash", "Index Scan"]);
    const [join, seq, hash, idx] = a.nodes;
    expect(join.exclusiveTimeMs).toBeCloseTo(1);
    expect(seq.exclusiveTimeMs).toBeCloseTo(7);
    expect(hash.exclusiveTimeMs).toBeCloseTo(0.5);
    expect(idx.exclusiveTimeMs).toBeCloseTo(1.5);
    expect(join.exclusiveCost).toBe(20);
    expect(a.totalTimeMs).toBe(10);
    expect(seq.share).toBeCloseTo(0.7);
    expect(seq.hot).toBe("high");
    expect(a.hottest?.id).toBe(seq.id);
    expect(seq.target).toBe("public.employees e");
    expect(idx.target).toBe("departments");
    expect(idx.index).toBe("departments_pkey");
    expect(seq.buffers).toMatchObject({ sharedHit: 12, sharedRead: 3, tempWritten: 0 });
    expect(join.relationship).toBeNull();
  });

  it("flags misestimates, filtered seq scans and spills", () => {
    const a = analyzePlan(analyzed, true);
    const [join, seq, hash] = a.nodes;
    expect(join.misestimate).toEqual({ factor: 100, direction: "under" });
    expect(a.worstMisestimate?.id).toBe(join.id);
    expect(join.warnings[0]).toMatch(/off by 100×/);
    expect(seq.warnings.some((w) => w.includes("discarded 95%"))).toBe(true);
    expect(hash.warnings.some((w) => w.includes("4 batches"))).toBe(true);
    expect(a.warningCount).toBe(3);
  });

  it("uses cost shares and no row actuals for plain EXPLAIN", () => {
    const plain: PlanNode = {
      "Node Type": "Sort",
      "Total Cost": 100,
      "Plan Rows": 10,
      "Sort Key": ["name"],
      Plans: [{ "Node Type": "Seq Scan", "Relation Name": "t", "Total Cost": 80, "Plan Rows": 10 }],
    };
    const a = analyzePlan(plain, false);
    expect(a.totalTimeMs).toBeNull();
    expect(a.nodes[0].actualRows).toBeNull();
    expect(a.nodes[0].misestimate).toBeNull();
    expect(a.nodes[1].share).toBeCloseTo(0.8);
    expect(a.nodes[0].details).toContainEqual(["Sort key", "name"]);
    expect(a.hottest?.id).toBe(1);
  });

  it("multiplies time by loops but not under Gather, and marks never-executed nodes", () => {
    const plan: PlanNode = {
      "Node Type": "Nested Loop",
      "Total Cost": 10,
      "Plan Rows": 1,
      "Actual Total Time": 5,
      "Actual Rows": 1,
      "Actual Loops": 1,
      Plans: [
        {
          "Node Type": "Gather",
          "Total Cost": 5,
          "Plan Rows": 1,
          "Actual Total Time": 1,
          "Actual Rows": 1,
          "Actual Loops": 1,
          Plans: [
            {
              "Node Type": "Seq Scan",
              "Parallel Aware": true,
              "Total Cost": 4,
              "Plan Rows": 1,
              "Actual Total Time": 0.8,
              "Actual Rows": 1,
              "Actual Loops": 3,
            },
          ],
        },
        {
          "Node Type": "Index Scan",
          "Total Cost": 1,
          "Plan Rows": 1,
          "Actual Total Time": 0.2,
          "Actual Rows": 1,
          "Actual Loops": 10,
        },
        {
          "Node Type": "Seq Scan",
          "Parent Relationship": "SubPlan",
          "Total Cost": 1,
          "Plan Rows": 1,
          "Actual Total Time": 0,
          "Actual Rows": 0,
          "Actual Loops": 0,
        },
      ],
    };
    const a = analyzePlan(plan, true);
    expect(a.nodes[2].inclusiveTimeMs).toBeCloseTo(0.8);
    expect(a.nodes[3].inclusiveTimeMs).toBeCloseTo(2);
    expect(a.nodes[0].exclusiveTimeMs).toBeCloseTo(2);
    expect(a.nodes[4].neverExecuted).toBe(true);
    expect(a.nodes[4].actualRows).toBeNull();
    expect(a.nodes[4].relationship).toBe("SubPlan");
  });

  it("attributes temp-file writes to the producing node only", () => {
    const blocks = (temp: number) => ({ "Shared Hit Blocks": 0, "Shared Read Blocks": 0, "Temp Written Blocks": temp });
    const plan: PlanNode = {
      "Node Type": "Aggregate",
      "Plan Rows": 1,
      ...blocks(40),
      Plans: [{ "Node Type": "Function Scan", "Plan Rows": 1000, ...blocks(40) }],
    };
    const a = analyzePlan(plan, false);
    expect(a.nodes[0].warnings).toEqual([]);
    expect(a.nodes[1].warnings).toEqual(["Node wrote temporary files to disk."]);
  });

  it("measures estimate error symmetrically", () => {
    expect(misestimateOf(10, 1000)).toEqual({ factor: 100, direction: "under" });
    expect(misestimateOf(1000, 10)).toEqual({ factor: 100, direction: "over" });
    expect(misestimateOf(0, 0).factor).toBe(1);
    expect(formatFactor(2.345)).toBe("2.3×");
    expect(formatFactor(1234)).toBe("1,234×");
  });
});
