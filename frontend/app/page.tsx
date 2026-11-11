"use client";

import { Database, Server, Terminal } from "lucide-react";

import { useHealth } from "@/hooks/use-health";
import { cn } from "@/lib/utils";

function StatusDot({ state }: { state: "ok" | "degraded" | "down" | "loading" }) {
  return (
    <span
      className={cn(
        "inline-block size-2 rounded-full",
        state === "ok" && "bg-success",
        state === "degraded" && "bg-warning",
        state === "down" && "bg-destructive",
        state === "loading" && "animate-pulse bg-muted-foreground",
      )}
    />
  );
}

export default function Home() {
  const { data, isPending, isError, error } = useHealth();

  const apiState = isPending ? "loading" : isError ? "down" : data.status === "ok" ? "ok" : "degraded";
  const dbState = isPending ? "loading" : isError ? "down" : data.database === "ok" ? "ok" : "down";

  return (
    <main className="flex h-full items-center justify-center p-6">
      <div className="w-full max-w-md space-y-6">
        <div className="flex items-center gap-2">
          <div className="flex size-8 items-center justify-center rounded-md bg-brand text-brand-foreground">
            <Terminal className="size-4" />
          </div>
          <h1 className="text-lg font-semibold tracking-tight">QueryLab</h1>
        </div>

        <div className="divide-y rounded-lg border bg-card">
          <div className="flex items-center justify-between px-4 py-3">
            <span className="flex items-center gap-2 text-sm">
              <Server className="size-4 text-muted-foreground" /> API
            </span>
            <span className="flex items-center gap-2 font-mono text-xs text-muted-foreground">
              <StatusDot state={apiState} />
              {isPending ? "checking…" : isError ? "unreachable" : `${data.status} · v${data.version}`}
            </span>
          </div>
          <div className="flex items-center justify-between px-4 py-3">
            <span className="flex items-center gap-2 text-sm">
              <Database className="size-4 text-muted-foreground" /> Metadata database
            </span>
            <span className="flex items-center gap-2 font-mono text-xs text-muted-foreground">
              <StatusDot state={dbState} />
              {isPending ? "checking…" : isError ? "unknown" : data.database}
            </span>
          </div>
        </div>

        {isError && (
          <p className="text-sm text-destructive">{error.message}</p>
        )}
      </div>
    </main>
  );
}
