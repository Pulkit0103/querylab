"use client";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useHealth } from "@/hooks/use-health";
import { cn } from "@/lib/utils";

/** Small API health indicator for the top bar. */
export function ApiStatus() {
  const { data, isPending, isError } = useHealth();
  const state = isPending ? "loading" : isError ? "down" : data.status === "ok" ? "ok" : "degraded";
  const label = {
    loading: "Checking API…",
    ok: `API online · v${data?.version ?? ""}`,
    degraded: "API online, metadata database unavailable",
    down: "API unreachable",
  }[state];

  return (
    <Tooltip>
      <TooltipTrigger
        render={<span className="flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-muted-foreground" />}
      >
        <span
          className={cn(
            "size-1.5 rounded-full",
            state === "ok" && "bg-success",
            state === "degraded" && "bg-warning",
            state === "down" && "bg-destructive",
            state === "loading" && "animate-pulse bg-muted-foreground",
          )}
        />
        <span className="hidden lg:inline">{state === "ok" ? "Online" : state === "loading" ? "…" : "Issues"}</span>
      </TooltipTrigger>
      <TooltipContent side="bottom">{label}</TooltipContent>
    </Tooltip>
  );
}
