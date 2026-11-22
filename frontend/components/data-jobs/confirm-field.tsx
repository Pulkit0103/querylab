"use client";

import type { Connection } from "@/types/api";

type Target = Pick<Connection, "name" | "environment">;

/** The `confirm` value sent with a privileged request. */
export function confirmValue(connection: Target, typed: string): string | undefined {
  if (connection.environment === "production") return typed;
  if (connection.environment === "staging") return "confirm";
  return undefined;
}

export function isConfirmed(connection: Target, typed: string): boolean {
  return connection.environment !== "production" || typed.trim() === connection.name.trim();
}

/** Typed confirmation for writes to production connections. */
export function ConfirmField({ connection, value, onChange }: { connection: Target; value: string; onChange: (v: string) => void }) {
  if (connection.environment !== "production") return null;
  return (
    <label className="block space-y-1 text-xs">
      <span>
        Type <strong className="font-mono">{connection.name}</strong> to confirm writing to this production database
      </span>
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        aria-label="Connection name"
        className="h-8 w-full max-w-sm rounded-md border bg-background px-2 font-mono text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring"
      />
    </label>
  );
}
