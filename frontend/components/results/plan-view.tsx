"use client";

import { GitBranch, Loader2, XCircle } from "lucide-react";

import { EmptyState } from "@/components/common/empty-state";
import { formatNumber } from "@/lib/format";
import type { TabRuntime } from "@/stores/editor-store";
import type { PlanNode } from "@/types/query";

function PlanNodeRow({ node, depth }: { node: PlanNode; depth: number }) {
  return (
    <li>
      <div className="flex items-baseline gap-3 rounded-md px-2 py-1 hover:bg-accent/40" style={{ paddingLeft: depth * 16 + 8 }}>
        <span className="font-medium">{node["Node Type"]}</span>
        {node["Relation Name"] && <span className="text-muted-foreground">on {node["Relation Name"]}</span>}
        <span className="ml-auto shrink-0 text-muted-foreground tabular-nums">
          cost {node["Total Cost"]?.toFixed(2) ?? "—"} · rows {formatNumber(node["Plan Rows"])}
        </span>
      </div>
      {node.Plans && node.Plans.length > 0 && (
        <ul>
          {node.Plans.map((child, i) => (
            <PlanNodeRow key={i} node={child} depth={depth + 1} />
          ))}
        </ul>
      )}
    </li>
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
        description="Click Explain to see how PostgreSQL will execute the current statement."
      />
    );
  }
  return (
    <ul className="h-full overflow-auto p-2 font-mono text-xs scrollbar-thin">
      <PlanNodeRow node={runtime.plan.plan} depth={0} />
    </ul>
  );
}
