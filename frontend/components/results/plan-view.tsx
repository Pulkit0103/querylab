"use client";

import { useMemo, useState, type ReactNode } from "react";
import {
  AlertTriangle,
  ArrowDownWideNarrow,
  Box,
  Braces,
  ChevronRight,
  Copy,
  Flame,
  GitBranch,
  GitMerge,
  KeyRound,
  ListTree,
  Loader2,
  Sigma,
  Table2,
  XCircle,
  type LucideIcon,
} from "lucide-react";
import { toast } from "sonner";

import { EmptyState } from "@/components/common/empty-state";
import { Button } from "@/components/ui/button";
import { formatDuration, formatNumber } from "@/lib/format";
import {
  analyzePlan,
  formatFactor,
  MISESTIMATE_THRESHOLD,
  type AnalyzedNode,
  type PlanAnalysis,
  type PlanCategory,
} from "@/lib/plan";
import { cn } from "@/lib/utils";
import type { TabRuntime } from "@/stores/editor-store";
import type { ExplainResult } from "@/types/query";

const CATEGORY_ICON: Record<PlanCategory, LucideIcon> = {
  scan: Table2,
  index: KeyRound,
  join: GitMerge,
  aggregate: Sigma,
  sort: ArrowDownWideNarrow,
  other: Box,
};

const CATEGORY_LABEL: Record<PlanCategory, string> = {
  scan: "Scan",
  index: "Index access",
  join: "Join",
  aggregate: "Aggregation",
  sort: "Sort",
  other: "Operation",
};

const formatCost = (c: number) => c.toLocaleString("en-US", { maximumFractionDigits: 2, minimumFractionDigits: 2 });
const formatPct = (share: number) => (share < 0.01 && share > 0 ? "<1%" : `${Math.round(share * 100)}%`);

function Stat({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-baseline gap-1.5">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium tabular-nums text-foreground">{children}</span>
    </div>
  );
}

function PlanSummary({
  result,
  analysis,
  onJump,
}: {
  result: ExplainResult;
  analysis: PlanAnalysis;
  onJump: (node: AnalyzedNode) => void;
}) {
  const { hottest, worstMisestimate } = analysis;
  return (
    <div className="flex flex-wrap items-center gap-x-5 gap-y-1.5 border-b bg-panel px-3 py-2 text-xs">
      <span
        className={cn(
          "rounded-full px-2 py-0.5 text-[10px] font-semibold tracking-wide uppercase",
          result.analyzed ? "bg-brand/15 text-brand" : "bg-muted text-muted-foreground",
        )}
      >
        {result.analyzed ? "Explain analyze" : "Explain · estimates"}
      </span>
      {result.analyzed && <Stat label="Execution">{formatDuration(result.execution_time_ms)}</Stat>}
      {result.planning_time_ms !== null && <Stat label="Planning">{formatDuration(result.planning_time_ms)}</Stat>}
      <Stat label="Total cost">{formatCost(analysis.totalCost)}</Stat>
      <Stat label="Nodes">{analysis.nodes.length}</Stat>
      {hottest && (
        <button
          type="button"
          onClick={() => onJump(hottest)}
          className="flex items-center gap-1 rounded px-1 text-destructive hover:bg-destructive/10"
        >
          <Flame className="size-3.5" />
          Slowest: {hottest.title} ({formatPct(hottest.share)} of {analysis.analyzed ? "time" : "cost"})
        </button>
      )}
      {worstMisestimate?.misestimate && (
        <button
          type="button"
          onClick={() => onJump(worstMisestimate)}
          className="flex items-center gap-1 rounded px-1 text-warning hover:bg-warning/10"
        >
          <AlertTriangle className="size-3.5" />
          Worst estimate: {formatFactor(worstMisestimate.misestimate.factor)} on {worstMisestimate.title}
        </button>
      )}
      {analysis.warningCount > 0 && (
        <span className="text-muted-foreground">
          {analysis.warningCount} {analysis.warningCount === 1 ? "warning" : "warnings"}
        </span>
      )}
    </div>
  );
}

function RowsCell({ n }: { n: AnalyzedNode }) {
  if (n.neverExecuted) return <span className="text-muted-foreground italic">never executed</span>;
  if (n.actualRows === null) {
    return (
      <span>
        <span className="text-muted-foreground">est </span>
        {formatNumber(n.estimatedRows)}
      </span>
    );
  }
  const flagged = n.misestimate && n.misestimate.factor >= MISESTIMATE_THRESHOLD;
  return (
    <span className="flex items-center gap-1">
      <span className="text-muted-foreground">{formatNumber(n.estimatedRows)} →</span>
      <span>{formatNumber(n.actualRows)}</span>
      {flagged && n.misestimate && (
        <span
          className="flex items-center gap-0.5 rounded bg-warning/15 px-1 text-[10px] text-warning"
          title={`Planner ${n.misestimate.direction === "under" ? "underestimated" : "overestimated"} rows by ${formatFactor(n.misestimate.factor)}`}
        >
          <AlertTriangle className="size-3" />
          {formatFactor(n.misestimate.factor)} {n.misestimate.direction}
        </span>
      )}
    </span>
  );
}

function NodeDetails({ n, analyzed }: { n: AnalyzedNode; analyzed: boolean }) {
  const rows: Array<[string, ReactNode]> = [
    ["Operation", CATEGORY_LABEL[n.category]],
    ["Cost", `${formatCost(Number(n.node["Startup Cost"] ?? 0))} … ${formatCost(n.totalCost)} (self ${formatCost(n.exclusiveCost)})`],
    ["Estimated rows", `${formatNumber(n.estimatedRows)} × width ${formatNumber(Number(n.node["Plan Width"] ?? 0))} B`],
  ];
  if (analyzed && !n.neverExecuted) {
    rows.push(
      ["Actual rows", `${formatNumber(n.actualRows)} per loop × ${formatNumber(n.loops)} loops`],
      [
        "Actual time",
        `${formatDuration(n.inclusiveTimeMs)} total · ${formatDuration(n.exclusiveTimeMs)} in this node (first row ${formatDuration(Number(n.node["Actual Startup Time"] ?? 0))})`,
      ],
    );
  }
  if (n.buffers) {
    const b = n.buffers;
    const parts = [`hit ${formatNumber(b.sharedHit)}`, `read ${formatNumber(b.sharedRead)}`];
    if (b.sharedDirtied) parts.push(`dirtied ${formatNumber(b.sharedDirtied)}`);
    if (b.sharedWritten) parts.push(`written ${formatNumber(b.sharedWritten)}`);
    if (b.tempRead || b.tempWritten) parts.push(`temp ${formatNumber(b.tempRead)} / ${formatNumber(b.tempWritten)}`);
    rows.push(["Buffers (incl. children)", `${parts.join(" · ")} pages`]);
  }
  for (const [k, v] of n.details) rows.push([k, v]);

  return (
    <div className="mb-1 ml-6 space-y-2 rounded-md border bg-panel/60 p-2.5">
      {n.warnings.length > 0 && (
        <ul className="space-y-1">
          {n.warnings.map((w) => (
            <li key={w} className="flex gap-1.5 text-warning">
              <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
              <span>{w}</span>
            </li>
          ))}
        </ul>
      )}
      <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1">
        {rows.map(([k, v]) => (
          <div key={k} className="contents">
            <dt className="text-muted-foreground">{k}</dt>
            <dd className="min-w-0 font-mono break-words whitespace-pre-wrap">{v}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

interface TreeState {
  collapsed: Set<number>;
  open: Set<number>;
  toggleCollapsed: (id: number) => void;
  toggleOpen: (id: number) => void;
}

function PlanNodeRow({ n, analyzed, tree }: { n: AnalyzedNode; analyzed: boolean; tree: TreeState }) {
  const Icon = CATEGORY_ICON[n.category];
  const collapsed = tree.collapsed.has(n.id);
  const open = tree.open.has(n.id);
  const hasChildren = n.children.length > 0;

  return (
    <li className="relative">
      <div
        id={`plan-node-${n.id}`}
        className={cn(
          "group relative rounded-md border border-transparent",
          n.hot === "high" && "border-destructive/40 bg-destructive/5",
          n.hot === "medium" && "border-warning/40 bg-warning/5",
          n.neverExecuted && "opacity-60",
        )}
      >
        <div className="flex items-center gap-1.5 px-1.5 py-1">
          <button
            type="button"
            aria-label={collapsed ? "Expand children" : "Collapse children"}
            aria-expanded={hasChildren ? !collapsed : undefined}
            disabled={!hasChildren}
            onClick={() => tree.toggleCollapsed(n.id)}
            className="grid size-4 shrink-0 place-items-center rounded text-muted-foreground hover:bg-accent disabled:invisible"
          >
            <ChevronRight className={cn("size-3.5 transition-transform", !collapsed && "rotate-90")} />
          </button>
          <button
            type="button"
            onClick={() => tree.toggleOpen(n.id)}
            aria-expanded={open}
            className="flex min-w-0 flex-1 items-center gap-2 text-left"
          >
            <Icon className="size-3.5 shrink-0 text-muted-foreground" aria-label={CATEGORY_LABEL[n.category]} />
            <span className="truncate">
              <span className="font-semibold">{n.title}</span>
              {n.relationship && (
                <span className="ml-1.5 rounded bg-muted px-1 text-[10px] text-muted-foreground">{n.relationship}</span>
              )}
              {n.index && <span className="text-muted-foreground"> using {n.index}</span>}
              {n.target && <span className="text-muted-foreground"> on {n.target}</span>}
            </span>
            {n.warnings.length > 0 && <AlertTriangle className="size-3.5 shrink-0 text-warning" aria-label="Has warnings" />}
            <span className="ml-auto flex shrink-0 items-center gap-4 pl-3 tabular-nums">
              {n.hot && (
                <span
                  className={cn("flex items-center gap-0.5", n.hot === "high" ? "text-destructive" : "text-warning")}
                >
                  <Flame className="size-3" /> {n.hot === "high" ? "hot" : "warm"}
                </span>
              )}
              <RowsCell n={n} />
              {analyzed && n.loops !== null && n.loops > 1 && (
                <span className="text-muted-foreground">×{formatNumber(n.loops)}</span>
              )}
              <span className="w-16 text-right">
                {analyzed ? formatDuration(n.exclusiveTimeMs) : formatCost(n.exclusiveCost)}
              </span>
              <span className="flex w-24 items-center gap-1.5" title={`${formatPct(n.share)} of ${analyzed ? "total time" : "total cost"}`}>
                <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
                  <span
                    className="block h-full rounded-full"
                    style={{ width: `${Math.max(n.share * 100, n.share > 0 ? 2 : 0)}%`, background: "var(--viz-1)" }}
                  />
                </span>
                <span className="w-8 text-right text-muted-foreground">{formatPct(n.share)}</span>
              </span>
            </span>
          </button>
        </div>
        {open && <NodeDetails n={n} analyzed={analyzed} />}
      </div>
      {hasChildren && !collapsed && (
        <ul className="ml-[13px] border-l border-border/70 pl-2">
          {n.children.map((child) => (
            <PlanNodeRow key={child.id} n={child} analyzed={analyzed} tree={tree} />
          ))}
        </ul>
      )}
    </li>
  );
}

function PlanExplorer({ result }: { result: ExplainResult }) {
  const analysis = useMemo(() => analyzePlan(result.plan, result.analyzed), [result]);
  const [mode, setMode] = useState<"tree" | "json">("tree");
  const [collapsed, setCollapsed] = useState<Set<number>>(() => new Set());
  // Open the hottest node by default so the first thing shown is where time goes.
  const [open, setOpen] = useState<Set<number>>(() => new Set(analysis.hottest ? [analysis.hottest.id] : []));
  const json = useMemo(() => JSON.stringify(result.raw, null, 2), [result.raw]);

  const toggle = (set: Set<number>, id: number) => {
    const next = new Set(set);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return next;
  };
  const tree: TreeState = {
    collapsed,
    open,
    toggleCollapsed: (id) => setCollapsed((s) => toggle(s, id)),
    toggleOpen: (id) => setOpen((s) => toggle(s, id)),
  };

  const jumpTo = (target: AnalyzedNode) => {
    setMode("tree");
    // Expand every ancestor so the node is visible, then open its details.
    const ancestors = new Set<number>();
    const find = (n: AnalyzedNode, path: number[]): boolean => {
      if (n.id === target.id) {
        path.forEach((id) => ancestors.add(id));
        return true;
      }
      return n.children.some((c) => find(c, [...path, n.id]));
    };
    find(analysis.root, []);
    setCollapsed((s) => new Set([...s].filter((id) => !ancestors.has(id))));
    setOpen((s) => new Set(s).add(target.id));
    requestAnimationFrame(() =>
      document.getElementById(`plan-node-${target.id}`)?.scrollIntoView({ block: "nearest", behavior: "smooth" }),
    );
  };

  return (
    <div className="flex h-full min-h-0 flex-col text-xs">
      <PlanSummary result={result} analysis={analysis} onJump={jumpTo} />
      <div className="flex shrink-0 items-center gap-1 border-b px-2 py-1">
        <Button
          variant={mode === "tree" ? "secondary" : "ghost"}
          size="xs"
          onClick={() => setMode("tree")}
          className="gap-1"
        >
          <ListTree className="size-3.5" /> Tree
        </Button>
        <Button
          variant={mode === "json" ? "secondary" : "ghost"}
          size="xs"
          onClick={() => setMode("json")}
          className="gap-1"
        >
          <Braces className="size-3.5" /> JSON
        </Button>
        {mode === "tree" && (
          <>
            <Button variant="ghost" size="xs" onClick={() => setCollapsed(new Set())}>
              Expand all
            </Button>
            <Button
              variant="ghost"
              size="xs"
              onClick={() => setOpen(new Set(analysis.nodes.map((n) => n.id)))}
            >
              Show all details
            </Button>
            <Button variant="ghost" size="xs" onClick={() => setOpen(new Set())}>
              Hide details
            </Button>
          </>
        )}
        <span className="ml-auto hidden text-muted-foreground md:inline">
          {result.analyzed ? "estimated → actual rows · self time · share of time" : "estimated rows · self cost · share of cost"}
        </span>
        <Button
          variant="ghost"
          size="xs"
          className="gap-1"
          onClick={() =>
            void navigator.clipboard.writeText(json).then(
              () => toast.success("Plan JSON copied"),
              () => toast.error("Clipboard access was denied by the browser."),
            )
          }
        >
          <Copy className="size-3.5" /> Copy JSON
        </Button>
      </div>
      {mode === "tree" ? (
        <ul className="min-h-0 flex-1 overflow-auto p-2 scrollbar-thin" aria-label="Execution plan">
          <PlanNodeRow n={analysis.root} analyzed={analysis.analyzed} tree={tree} />
        </ul>
      ) : (
        <pre className="min-h-0 flex-1 overflow-auto p-3 font-mono text-[11px] leading-relaxed scrollbar-thin">{json}</pre>
      )}
    </div>
  );
}

export function PlanView({ runtime }: { runtime: TabRuntime }) {
  if (runtime.exec.status === "running" && runtime.exec.kind === "explain") {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-xs text-muted-foreground">
        <Loader2 className="size-4 animate-spin text-brand" /> Building execution plan…
      </div>
    );
  }
  if (runtime.planError) {
    return <EmptyState icon={XCircle} title="Could not explain query" description={runtime.planError.message} />;
  }
  if (!runtime.plan) {
    return (
      <EmptyState
        icon={GitBranch}
        title="No execution plan"
        description="Click Explain to see how PostgreSQL will execute the current statement, or Explain Analyze to measure it."
      />
    );
  }
  // Remount per plan so expansion state starts fresh for each EXPLAIN.
  return <PlanExplorer key={planKey(runtime.plan)} result={runtime.plan} />;
}

const planKeys = new WeakMap<ExplainResult, number>();
let nextPlanKey = 0;
function planKey(plan: ExplainResult): number {
  let key = planKeys.get(plan);
  if (key === undefined) {
    key = nextPlanKey++;
    planKeys.set(plan, key);
  }
  return key;
}
