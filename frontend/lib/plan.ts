/**
 * Analysis of PostgreSQL `EXPLAIN (FORMAT JSON)` plans: derives per-node
 * exclusive time/cost, row-estimate accuracy, hot spots and warnings that
 * the plan visualization renders. Pure functions, no React.
 */

import type { PlanNode } from "@/types/query";

export type PlanCategory = "scan" | "index" | "join" | "aggregate" | "sort" | "other";

export type HotLevel = "high" | "medium" | null;

export interface Misestimate {
  /** How far off the estimate was, always >= 1 (e.g. 120 = 120×). */
  factor: number;
  /** `under`: more rows than the planner expected; `over`: fewer. */
  direction: "under" | "over";
}

export interface BufferStats {
  sharedHit: number;
  sharedRead: number;
  sharedDirtied: number;
  sharedWritten: number;
  tempRead: number;
  tempWritten: number;
}

export interface AnalyzedNode {
  /** Pre-order index, stable for a given plan. */
  id: number;
  depth: number;
  node: PlanNode;
  children: AnalyzedNode[];
  /** Display name, e.g. `Hash Left Join`, `Parallel Seq Scan`. */
  title: string;
  /** What the node operates on, e.g. `public.employees e`. */
  target: string | null;
  /** Index used, if any. */
  index: string | null;
  /** `InitPlan`, `SubPlan`, … when the node is not a plain child. */
  relationship: string | null;
  category: PlanCategory;
  totalCost: number;
  /** Cost attributable to this node alone (total minus children). */
  exclusiveCost: number;
  /** Planner row estimate (per loop). */
  estimatedRows: number;
  /** Actual rows per loop (ANALYZE only). */
  actualRows: number | null;
  loops: number | null;
  /** Inclusive wall time across all loops in ms (ANALYZE only). */
  inclusiveTimeMs: number | null;
  /** Time spent in this node alone in ms (ANALYZE only). */
  exclusiveTimeMs: number | null;
  /** Share (0-1) of total time (ANALYZE) or total cost (plain EXPLAIN). */
  share: number;
  hot: HotLevel;
  misestimate: Misestimate | null;
  neverExecuted: boolean;
  buffers: BufferStats | null;
  /** Ordered key/value details worth showing (conditions, sort keys, …). */
  details: Array<[string, string]>;
  warnings: string[];
}

export interface PlanAnalysis {
  root: AnalyzedNode;
  /** All nodes in pre-order. */
  nodes: AnalyzedNode[];
  analyzed: boolean;
  totalCost: number;
  /** Inclusive time of the root node in ms (ANALYZE only). */
  totalTimeMs: number | null;
  /** Node with the largest share, if any node stands out. */
  hottest: AnalyzedNode | null;
  /** Node with the worst row misestimate (factor >= 10), if any. */
  worstMisestimate: AnalyzedNode | null;
  warningCount: number;
}

/** Misestimates at or above this factor are flagged. */
export const MISESTIMATE_THRESHOLD = 10;

const SCAN = new Set([
  "Seq Scan",
  "Sample Scan",
  "Tid Scan",
  "Tid Range Scan",
  "Function Scan",
  "Table Function Scan",
  "Values Scan",
  "CTE Scan",
  "Named Tuplestore Scan",
  "Subquery Scan",
  "WorkTable Scan",
  "Foreign Scan",
  "Custom Scan",
  "Result",
]);
const INDEX = new Set(["Index Scan", "Index Only Scan", "Bitmap Index Scan", "Bitmap Heap Scan"]);
const JOIN = new Set(["Nested Loop", "Hash Join", "Merge Join", "Hash"]);
const AGGREGATE = new Set(["Aggregate", "Group", "WindowAgg", "Unique", "SetOp"]);
const SORT = new Set(["Sort", "Incremental Sort", "Gather Merge"]);

export function categoryOf(nodeType: string): PlanCategory {
  if (INDEX.has(nodeType)) return "index";
  if (SCAN.has(nodeType)) return "scan";
  if (JOIN.has(nodeType)) return "join";
  if (AGGREGATE.has(nodeType)) return "aggregate";
  if (SORT.has(nodeType)) return "sort";
  return "other";
}

const num = (v: unknown): number | null => (typeof v === "number" && Number.isFinite(v) ? v : null);
const str = (v: unknown): string | null => (typeof v === "string" && v.length > 0 ? v : null);

/** Display title in the style of PostgreSQL's text format. */
export function nodeTitle(node: PlanNode): string {
  let type = node["Node Type"];
  if (type === "Aggregate") {
    const strategy = str(node["Strategy"]);
    if (strategy === "Hashed") type = "HashAggregate";
    else if (strategy === "Sorted") type = "GroupAggregate";
    else if (strategy === "Mixed") type = "MixedAggregate";
  }
  if (type === "SetOp" && str(node["Strategy"]) === "Hashed") type = "HashSetOp";
  const join = str(node["Join Type"]);
  if (join && join !== "Inner") {
    if (type === "Nested Loop") type = `Nested Loop ${join} Join`;
    else if (type.endsWith(" Join")) type = `${type.slice(0, -" Join".length)} ${join} Join`;
  }
  const partial = str(node["Partial Mode"]);
  if (partial && partial !== "Simple") type = `${partial} ${type}`;
  if (node["Parallel Aware"] === true) type = `Parallel ${type}`;
  return type;
}

function nodeTarget(node: PlanNode): string | null {
  const relation = str(node["Relation Name"]);
  if (relation) {
    const schema = str(node["Schema"]);
    const alias = str(node["Alias"]);
    const qualified = schema ? `${schema}.${relation}` : relation;
    return alias && alias !== relation ? `${qualified} ${alias}` : qualified;
  }
  const cte = str(node["CTE Name"]);
  if (cte) return cte;
  const fn = str(node["Function Name"]);
  if (fn) return `${fn}()`;
  const alias = str(node["Alias"]);
  return alias;
}

function formatValue(v: unknown): string | null {
  if (v === null || v === undefined || v === "") return null;
  if (Array.isArray(v)) return v.length ? v.map(String).join(", ") : null;
  if (typeof v === "number") return Number.isInteger(v) ? v.toLocaleString("en-US") : String(v);
  if (typeof v === "boolean") return v ? "yes" : "no";
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}

/** Keys rendered as details, in display order. */
const DETAIL_KEYS: Array<[string, string]> = [
  ["Index Cond", "Index condition"],
  ["Recheck Cond", "Recheck condition"],
  ["Hash Cond", "Hash condition"],
  ["Merge Cond", "Merge condition"],
  ["Join Filter", "Join filter"],
  ["Filter", "Filter"],
  ["One-Time Filter", "One-time filter"],
  ["Rows Removed by Filter", "Rows removed by filter"],
  ["Rows Removed by Join Filter", "Rows removed by join filter"],
  ["Rows Removed by Index Recheck", "Rows removed by recheck"],
  ["Heap Fetches", "Heap fetches"],
  ["Scan Direction", "Scan direction"],
  ["Sort Key", "Sort key"],
  ["Presorted Key", "Presorted key"],
  ["Sort Method", "Sort method"],
  ["Group Key", "Group key"],
  ["Hash Buckets", "Hash buckets"],
  ["Hash Batches", "Hash batches"],
  ["Peak Memory Usage", "Peak memory (kB)"],
  ["Workers Planned", "Workers planned"],
  ["Workers Launched", "Workers launched"],
  ["Subplan Name", "Subplan"],
  ["Output", "Output"],
];

function nodeDetails(node: PlanNode): Array<[string, string]> {
  const out: Array<[string, string]> = [];
  for (const [key, label] of DETAIL_KEYS) {
    const value = formatValue(node[key]);
    if (value !== null) out.push([label, value]);
  }
  const space = num(node["Sort Space Used"]);
  if (space !== null) out.push(["Sort space", `${space.toLocaleString("en-US")} kB (${str(node["Sort Space Type"]) ?? "?"})`]);
  return out;
}

function nodeBuffers(node: PlanNode): BufferStats | null {
  const keys = {
    sharedHit: "Shared Hit Blocks",
    sharedRead: "Shared Read Blocks",
    sharedDirtied: "Shared Dirtied Blocks",
    sharedWritten: "Shared Written Blocks",
    tempRead: "Temp Read Blocks",
    tempWritten: "Temp Written Blocks",
  } as const;
  if (num(node[keys.sharedHit]) === null && num(node[keys.sharedRead]) === null) return null;
  const stats = {} as BufferStats;
  for (const [field, key] of Object.entries(keys) as Array<[keyof BufferStats, string]>) {
    stats[field] = num(node[key]) ?? 0;
  }
  return stats;
}

export function misestimateOf(estimated: number, actual: number): Misestimate {
  const ratio = Math.max(actual, 1) / Math.max(estimated, 1);
  return ratio >= 1 ? { factor: ratio, direction: "under" } : { factor: 1 / ratio, direction: "over" };
}

export function formatFactor(f: number): string {
  return f >= 100 ? `${Math.round(f).toLocaleString("en-US")}×` : `${f.toFixed(f < 10 ? 1 : 0)}×`;
}

function nodeWarnings(n: AnalyzedNode): string[] {
  const { node } = n;
  const warnings: string[] = [];
  if (n.misestimate && n.misestimate.factor >= MISESTIMATE_THRESHOLD) {
    warnings.push(
      `Row estimate off by ${formatFactor(n.misestimate.factor)} (${n.misestimate.direction === "under" ? "underestimated" : "overestimated"}) — table statistics may be stale; try ANALYZE.`,
    );
  }
  const removed = num(node["Rows Removed by Filter"]);
  if (node["Node Type"] === "Seq Scan" && removed !== null && n.actualRows !== null) {
    const loops = n.loops ?? 1;
    const discarded = removed * loops;
    const fraction = discarded / (discarded + n.actualRows * loops);
    if (discarded >= 1000 && fraction >= 0.9) {
      warnings.push(
        `Sequential scan discarded ${Math.round(fraction * 100)}% of rows by filter — an index on the filtered columns may help.`,
      );
    }
  }
  if (str(node["Sort Space Type"]) === "Disk") {
    warnings.push("Sort spilled to disk — consider raising work_mem or reducing the sorted set.");
  }
  const batches = num(node["Hash Batches"]);
  if (batches !== null && batches > 1) {
    warnings.push(`Hash table split into ${batches} batches (spilled to disk) — consider raising work_mem.`);
  }
  // Buffer counters are cumulative over the subtree; attribute temp writes
  // only to the node that produced them.
  const childTemp = n.children.reduce((s, c) => s + (c.buffers?.tempWritten ?? 0), 0);
  if (n.buffers && n.buffers.tempWritten > childTemp && !warnings.some((w) => w.includes("disk"))) {
    warnings.push("Node wrote temporary files to disk.");
  }
  return warnings;
}

const PARALLEL_PARENTS = new Set(["Gather", "Gather Merge"]);

/**
 * Builds the analyzed tree. Exclusive time is the node's inclusive time
 * minus that of its children; inclusive time is per-loop time × loops,
 * except beneath Gather nodes where loops are concurrent workers (PostgreSQL
 * reports per-worker averages there).
 */
export function analyzePlan(plan: PlanNode, analyzed: boolean): PlanAnalysis {
  const nodes: AnalyzedNode[] = [];

  const build = (node: PlanNode, depth: number, parallel: boolean): AnalyzedNode => {
    const loops = analyzed ? num(node["Actual Loops"]) : null;
    const perLoop = analyzed ? num(node["Actual Total Time"]) : null;
    const actualRows = analyzed && loops !== 0 ? num(node["Actual Rows"]) : null;
    const estimatedRows = num(node["Plan Rows"]) ?? 0;
    const inclusiveTimeMs = perLoop === null ? null : parallel ? perLoop : perLoop * (loops ?? 1);
    const n: AnalyzedNode = {
      id: nodes.length,
      depth,
      node,
      children: [],
      title: nodeTitle(node),
      target: nodeTarget(node),
      index: str(node["Index Name"]),
      relationship: (() => {
        const rel = str(node["Parent Relationship"]);
        return rel && rel !== "Outer" && rel !== "Inner" && rel !== "Member" ? rel : null;
      })(),
      category: categoryOf(node["Node Type"]),
      totalCost: num(node["Total Cost"]) ?? 0,
      exclusiveCost: 0,
      estimatedRows,
      actualRows,
      loops,
      inclusiveTimeMs,
      exclusiveTimeMs: null,
      share: 0,
      hot: null,
      misestimate: actualRows !== null ? misestimateOf(estimatedRows, actualRows) : null,
      neverExecuted: analyzed && loops === 0,
      buffers: nodeBuffers(node),
      details: nodeDetails(node),
      warnings: [],
    };
    nodes.push(n);
    const childParallel = parallel || PARALLEL_PARENTS.has(node["Node Type"]);
    n.children = (node.Plans ?? []).map((child) => build(child, depth + 1, childParallel));

    // InitPlans run once and are not included in the parent's cost.
    const costChildren = n.children.filter((c) => c.relationship !== "InitPlan");
    n.exclusiveCost = Math.max(0, n.totalCost - costChildren.reduce((s, c) => s + c.totalCost, 0));
    if (inclusiveTimeMs !== null) {
      const childTime = n.children.reduce((s, c) => s + (c.inclusiveTimeMs ?? 0), 0);
      n.exclusiveTimeMs = Math.max(0, inclusiveTimeMs - childTime);
    }
    n.warnings = nodeWarnings(n);
    return n;
  };

  const root = build(plan, 0, false);
  const totalCost = root.totalCost;
  const totalTimeMs = analyzed ? root.inclusiveTimeMs : null;
  const timeBase = Math.max(totalTimeMs ?? 0, nodes.reduce((s, n) => s + (n.exclusiveTimeMs ?? 0), 0));
  const costBase = Math.max(totalCost, nodes.reduce((s, n) => s + n.exclusiveCost, 0));

  let hottest: AnalyzedNode | null = null;
  let worstMisestimate: AnalyzedNode | null = null;
  for (const n of nodes) {
    const share = analyzed && timeBase > 0 ? (n.exclusiveTimeMs ?? 0) / timeBase : costBase > 0 ? n.exclusiveCost / costBase : 0;
    n.share = Math.min(1, share);
    n.hot = n.share >= 0.5 ? "high" : n.share >= 0.2 ? "medium" : null;
    if (n.hot && (!hottest || n.share > hottest.share)) hottest = n;
    if (n.misestimate && n.misestimate.factor >= MISESTIMATE_THRESHOLD) {
      if (!worstMisestimate || n.misestimate.factor > (worstMisestimate.misestimate?.factor ?? 0)) worstMisestimate = n;
    }
  }

  return {
    root,
    nodes,
    analyzed,
    totalCost,
    totalTimeMs,
    hottest: nodes.length > 1 ? hottest : null,
    worstMisestimate,
    warningCount: nodes.reduce((s, n) => s + n.warnings.length, 0),
  };
}
