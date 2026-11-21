/** Side-by-side comparison of two analysed plans. Pure functions. */
import type { AnalyzedNode, PlanAnalysis } from "@/lib/plan";

export interface MetricDiff {
  label: string;
  unit: "ms" | "cost" | "count";
  before: number | null;
  after: number | null;
  /** after / before; < 1 is an improvement for every metric here. */
  ratio: number | null;
}

export interface AccessChange {
  /** Relation as shown in the plan, e.g. `sales.orders o`. */
  target: string;
  before: string[];
  after: string[];
  changed: boolean;
}

export interface PlanComparison {
  metrics: MetricDiff[];
  /** How each relation is read in each plan. */
  access: AccessChange[];
  /** Operation counts (by title) that differ. */
  operations: Array<{ title: string; before: number; after: number }>;
}

const metric = (label: string, unit: MetricDiff["unit"], before: number | null, after: number | null): MetricDiff => ({
  label,
  unit,
  before,
  after,
  ratio: before !== null && after !== null && before > 0 ? after / before : null,
});

/** How a node reads its relation, e.g. `Index Scan using orders_pkey`. */
export function accessMethod(n: AnalyzedNode): string {
  return n.index ? `${n.title} using ${n.index}` : n.title;
}

function accessByTarget(nodes: AnalyzedNode[]): Map<string, string[]> {
  const out = new Map<string, string[]>();
  for (const n of nodes) {
    if (!n.target || (n.category !== "scan" && n.category !== "index")) continue;
    // Bitmap Index Scans name the index, not the table; their heap scan
    // carries the relation.
    const list = out.get(n.target) ?? [];
    list.push(accessMethod(n));
    out.set(n.target, list);
  }
  return out;
}

function counts(nodes: AnalyzedNode[]): Map<string, number> {
  const out = new Map<string, number>();
  for (const n of nodes) out.set(n.title, (out.get(n.title) ?? 0) + 1);
  return out;
}

export function comparePlans(before: PlanAnalysis, after: PlanAnalysis): PlanComparison {
  const metrics = [
    metric("Execution time", "ms", before.totalTimeMs, after.totalTimeMs),
    metric("Total cost", "cost", before.totalCost, after.totalCost),
    metric("Nodes", "count", before.nodes.length, after.nodes.length),
  ];

  const a = accessByTarget(before.nodes);
  const b = accessByTarget(after.nodes);
  const targets = [...new Set([...a.keys(), ...b.keys()])].sort();
  const access = targets.map((target) => {
    const x = (a.get(target) ?? []).slice().sort();
    const y = (b.get(target) ?? []).slice().sort();
    return { target, before: x, after: y, changed: x.join("|") !== y.join("|") };
  });

  const ca = counts(before.nodes);
  const cb = counts(after.nodes);
  const operations = [...new Set([...ca.keys(), ...cb.keys()])]
    .map((title) => ({ title, before: ca.get(title) ?? 0, after: cb.get(title) ?? 0 }))
    .filter((o) => o.before !== o.after)
    .sort((x, y) => x.title.localeCompare(y.title));

  return { metrics, access, operations };
}

/** `2.4× faster`, `1.3× slower`, `same`, or null when not comparable. */
export function describeRatio(ratio: number | null, unit: MetricDiff["unit"]): string | null {
  if (ratio === null || !Number.isFinite(ratio)) return null;
  if (Math.abs(ratio - 1) < 0.005) return "same";
  if (unit === "count") return ratio < 1 ? "fewer" : "more";
  const factor = ratio < 1 ? 1 / ratio : ratio;
  const better = unit === "ms" ? (ratio < 1 ? "faster" : "slower") : ratio < 1 ? "cheaper" : "costlier";
  return `${factor >= 10 ? Math.round(factor) : factor.toFixed(1)}× ${better}`;
}
