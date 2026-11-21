"use client";

import { useMemo } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { ArrowRight, GitCompare, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { EmptyState } from "@/components/common/empty-state";
import { PlanExplorer } from "@/components/results/plan-view";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useDeletePlan, useSavedPlan, useSavedPlans } from "@/hooks/use-performance";
import { formatDuration, formatNumber, formatRelativeTime } from "@/lib/format";
import { analyzePlan } from "@/lib/plan";
import { comparePlans, describeRatio, type MetricDiff } from "@/lib/plan-compare";
import { cn } from "@/lib/utils";
import type { SavedPlan } from "@/types/performance";

const formatMetric = (v: number | null, unit: MetricDiff["unit"]) =>
  v === null ? "—" : unit === "ms" ? formatDuration(v) : unit === "cost" ? formatNumber(Math.round(v * 100) / 100) : formatNumber(v);

function Comparison({ before, after }: { before: SavedPlan; after: SavedPlan }) {
  const result = useMemo(
    () => comparePlans(analyzePlan(before.plan.plan, before.plan.analyzed), analyzePlan(after.plan.plan, after.plan.analyzed)),
    [before, after],
  );
  const changed = result.access.filter((a) => a.changed);
  return (
    <section className="space-y-3 border-b p-3 text-xs" aria-label="Comparison">
      <div className="flex flex-wrap gap-3">
        {result.metrics.map((m) => {
          const verdict = describeRatio(m.ratio, m.unit);
          const better = m.ratio !== null && m.ratio < 0.995;
          const worse = m.ratio !== null && m.ratio > 1.005;
          return (
            <div key={m.label} className="min-w-44 rounded-lg border bg-panel px-3 py-2">
              <p className="text-muted-foreground">{m.label}</p>
              <p className="flex items-center gap-1.5 font-medium tabular-nums">
                {formatMetric(m.before, m.unit)} <ArrowRight className="size-3 text-muted-foreground" /> {formatMetric(m.after, m.unit)}
              </p>
              {verdict && m.unit !== "count" && (
                <p className={cn(better && "text-success", worse && "text-destructive", !better && !worse && "text-muted-foreground")}>
                  {verdict}
                </p>
              )}
              {m.label === "Execution time" && m.before === null && m.after === null && (
                <p className="text-muted-foreground">Save plans with Explain Analyze to compare times.</p>
              )}
            </div>
          );
        })}
      </div>
      <div className="grid gap-3 md:grid-cols-2">
        <div>
          <h4 className="mb-1 font-medium text-muted-foreground">How tables are read</h4>
          {result.access.length === 0 ? (
            <p className="text-muted-foreground">No table access in either plan.</p>
          ) : (
            <ul className="space-y-1">
              {result.access.map((a) => (
                <li key={a.target} className={cn("rounded px-2 py-1", a.changed ? "bg-warning/10" : "bg-muted/40")}>
                  <span className="font-mono">{a.target}</span>:{" "}
                  <span className="text-muted-foreground">{a.before.join(", ") || "not read"}</span>
                  {a.changed && (
                    <>
                      {" "}
                      <ArrowRight className="inline size-3" /> <span>{a.after.join(", ") || "not read"}</span>
                    </>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
        <div>
          <h4 className="mb-1 font-medium text-muted-foreground">Operations that changed</h4>
          {result.operations.length === 0 ? (
            <p className="text-muted-foreground">{changed.length === 0 ? "The plans have the same shape." : "Same operations, different tables."}</p>
          ) : (
            <ul className="space-y-1">
              {result.operations.map((o) => (
                <li key={o.title} className="flex gap-2">
                  <span className="font-mono">{o.title}</span>
                  <span className="text-muted-foreground tabular-nums">
                    {o.before} → {o.after}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </section>
  );
}

function PlanColumn({ label, plan }: { label: string; plan: SavedPlan }) {
  return (
    <div className="flex min-h-[28rem] min-w-0 flex-col border-b lg:border-r lg:border-b-0">
      <div className="shrink-0 border-b bg-panel px-3 py-1.5 text-xs">
        <span className="mr-2 rounded bg-muted px-1.5 py-0.5 text-[10px] font-semibold tracking-wide uppercase">{label}</span>
        <span className="font-medium">{plan.name}</span>
        <span className="text-muted-foreground"> · {plan.connection_name ?? "deleted connection"} · {formatRelativeTime(plan.created_at)}</span>
      </div>
      <div className="min-h-0 flex-1">
        <PlanExplorer result={plan.plan} />
      </div>
    </div>
  );
}

/** Saved plans, and a side-by-side comparison of two of them. */
export function PlansPanel() {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const plans = useSavedPlans();
  const remove = useDeletePlan();
  const a = params.get("a");
  const b = params.get("b");
  const before = useSavedPlan(a);
  const after = useSavedPlan(b);

  const setSelection = (next: { a: string | null; b: string | null }) => {
    const q = new URLSearchParams(params.toString());
    for (const key of ["a", "b"] as const) {
      if (next[key]) q.set(key, next[key]!);
      else q.delete(key);
    }
    router.replace(`${pathname}?${q.toString()}`);
  };

  /** Clicking a plan fills "before" first, then "after"; clicking again clears it. */
  const toggle = (id: string) => {
    if (a === id) setSelection({ a: b, b: null });
    else if (b === id) setSelection({ a, b: null });
    else if (!a) setSelection({ a: id, b });
    else setSelection({ a, b: id });
  };

  if (plans.isError) return <p className="p-4 text-sm text-destructive">{plans.error.message}</p>;
  if (!plans.data) return <Skeleton className="m-3 h-40" />;
  if (plans.data.length === 0)
    return (
      <EmptyState
        icon={GitCompare}
        title="No saved plans"
        description="Run Explain or Explain Analyze in the SQL editor, then use “Save plan” in the Execution Plan tab. Save a plan before and after a change (for example adding an index) to compare them here."
      />
    );

  return (
    <div className="flex h-full min-h-0 flex-col lg:flex-row">
      <aside className="max-h-64 shrink-0 overflow-auto border-b scrollbar-thin lg:max-h-none lg:w-80 lg:border-r lg:border-b-0" aria-label="Saved plans">
        <p className="border-b px-3 py-2 text-xs text-muted-foreground">Pick two plans: the first is “before”, the second “after”.</p>
        <ul className="divide-y text-xs">
          {plans.data.map((p) => {
            const role = p.id === a ? "Before" : p.id === b ? "After" : null;
            return (
              <li key={p.id} className={cn("group flex items-start gap-2 px-3 py-2", role && "bg-accent/60")}>
                <button type="button" className="min-w-0 flex-1 space-y-0.5 text-left" onClick={() => toggle(p.id)} aria-pressed={role !== null}>
                  <span className="flex items-center gap-2">
                    {role && <span className="rounded bg-brand/15 px-1.5 text-[10px] font-semibold text-brand uppercase">{role}</span>}
                    <span className="truncate font-medium">{p.name}</span>
                  </span>
                  <span className="block truncate font-mono text-[11px] text-muted-foreground">{p.sql_preview}</span>
                  <span className="block text-[11px] text-muted-foreground">
                    {p.analyzed ? `analyze · ${formatDuration(p.execution_time_ms)}` : "estimate"} · cost{" "}
                    {formatNumber(p.total_cost === null ? null : Math.round(p.total_cost))} · {formatRelativeTime(p.created_at)}
                  </span>
                </button>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={`Delete plan ${p.name}`}
                  className="opacity-60 group-hover:opacity-100"
                  onClick={() =>
                    remove.mutate(p.id, {
                      onSuccess: () => {
                        if (p.id === a || p.id === b) setSelection({ a: a === p.id ? b : a, b: null });
                        toast.success("Plan deleted");
                      },
                      onError: (e) => toast.error(e.message),
                    })
                  }
                >
                  <Trash2 className="size-3.5" />
                </Button>
              </li>
            );
          })}
        </ul>
      </aside>
      <div className="min-h-0 min-w-0 flex-1 overflow-auto scrollbar-thin">
        {!a ? (
          <EmptyState icon={GitCompare} title="Choose plans to compare" description="Select a saved plan on the left, then a second one." />
        ) : before.isError || after.isError ? (
          <p className="p-4 text-sm text-destructive">{(before.error ?? after.error)?.message}</p>
        ) : !before.data || (b && !after.data) ? (
          <Skeleton className="m-3 h-64" />
        ) : (
          <>
            {after.data && b && <Comparison before={before.data} after={after.data} />}
            <div className={cn("grid", after.data && b && "lg:grid-cols-2")}>
              <PlanColumn label="Before" plan={before.data} />
              {after.data && b && <PlanColumn label="After" plan={after.data} />}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
