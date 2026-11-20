"use client";

import { AlertTriangle, ShieldCheck } from "lucide-react";

import { useActiveConnection } from "@/hooks/use-connections";
import { cn } from "@/lib/utils";

/**
 * Persistent strip shown while the active connection is tagged staging or
 * production, so it is never ambiguous which environment queries hit.
 */
export function EnvironmentBanner() {
  const { connection } = useActiveConnection();
  if (!connection || connection.environment === "development") return null;
  const production = connection.environment === "production";
  return (
    <div
      role="status"
      className={cn(
        "flex shrink-0 items-center gap-2 border-b px-4 py-1 text-xs",
        production ? "border-destructive/30 bg-destructive/10 text-destructive" : "border-warning/30 bg-warning/10 text-warning",
      )}
    >
      <AlertTriangle className="size-3.5 shrink-0" />
      <span className="truncate">
        <strong className="font-semibold">{production ? "Production" : "Staging"}</strong> database “{connection.name}”
        {connection.read_only ? " · read-only" : " · read-write: statements can modify data"}
      </span>
      {connection.read_only && <ShieldCheck className="size-3.5 shrink-0" aria-hidden />}
    </div>
  );
}
