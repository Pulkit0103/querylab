"use client";

import { useEffect, useState } from "react";
import { CheckCircle2, Loader2, ShieldAlert, XCircle } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useRunMigration } from "@/hooks/use-migrations";
import { formatDuration } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { Connection } from "@/types/api";
import type { Migration, RunOutcome } from "@/types/migrations";

export type RunDirection = "apply" | "rollback";

function Outcome({ outcome, label }: { outcome: RunOutcome; label: string }) {
  const ok = outcome.status === "success";
  return (
    <div className={cn("rounded-md border p-3 text-xs", ok ? "border-success/40 bg-success/5" : "border-destructive/40 bg-destructive/5")} role="status">
      <p className="flex items-center gap-1.5 font-medium">
        {ok ? <CheckCircle2 className="size-4 text-success" /> : <XCircle className="size-4 text-destructive" />}
        {label}: {ok ? "succeeded" : "failed"} in {formatDuration(outcome.duration_ms)}
        {outcome.dry_run && " (rolled back)"}
      </p>
      {outcome.error && (
        <div className="mt-1 space-y-0.5">
          <p className="font-mono text-destructive">{outcome.error.message}</p>
          {outcome.error.detail && <p className="text-muted-foreground">{outcome.error.detail}</p>}
          {outcome.error.hint && <p className="text-muted-foreground">Hint: {outcome.error.hint}</p>}
        </div>
      )}
    </div>
  );
}

/**
 * Apply or roll back a migration: a dry run (the script runs and is rolled
 * back) first, then the real run after confirmation.
 */
export function RunDialog({
  migration,
  direction,
  connection,
  onClose,
}: {
  migration: Migration | null;
  direction: RunDirection;
  connection: Pick<Connection, "id" | "name" | "environment" | "read_only">;
  onClose: () => void;
}) {
  const run = useRunMigration(migration?.project_id ?? "");
  const [preview, setPreview] = useState<RunOutcome | null>(null);
  const [blocked, setBlocked] = useState<string | null>(null);
  const [result, setResult] = useState<RunOutcome | null>(null);
  const [typed, setTyped] = useState("");
  const production = connection.environment === "production";
  const verb = direction === "apply" ? "Apply" : "Roll back";
  const script = direction === "apply" ? migration?.up_sql : migration?.down_sql;
  const { mutate } = run;

  // Preview as soon as the dialog opens (keyed per migration by the caller).
  useEffect(() => {
    if (!migration || connection.read_only) return;
    mutate(
      { id: migration.id, direction, connection_id: connection.id, dry_run: true },
      { onSuccess: setPreview, onError: (e) => setBlocked(e.message) },
    );
  }, [migration, direction, connection.id, connection.read_only, mutate]);

  const execute = () => {
    if (!migration) return;
    run.mutate(
      {
        id: migration.id,
        direction,
        connection_id: connection.id,
        confirm: connection.environment === "development" ? undefined : typed || "confirm",
      },
      {
        onSuccess: (outcome) => {
          setResult(outcome);
          if (outcome.status === "success") toast.success(`Migration ${migration.version} ${direction === "apply" ? "applied" : "rolled back"}`);
        },
        onError: (e) => toast.error(e.message),
      },
    );
  };

  return (
    <Dialog open={migration !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ShieldAlert className={cn("size-4", production ? "text-destructive" : "text-warning")} />
            {verb} {migration?.version} on {connection.name}
          </DialogTitle>
          <DialogDescription>
            {migration?.name} · {connection.environment}. The script runs in one transaction with an advisory lock, so a
            failure changes nothing.
          </DialogDescription>
        </DialogHeader>
        <pre className="max-h-56 overflow-auto rounded-md border bg-panel p-2 font-mono text-[11px] whitespace-pre-wrap">{script}</pre>
        {connection.read_only ? (
          <p className="text-sm text-destructive">This connection is read-only. Enable read-write mode in its settings to run migrations.</p>
        ) : blocked ? (
          <p className="text-sm text-destructive" role="alert">
            {blocked}
          </p>
        ) : !preview ? (
          <p className="flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin" /> Dry run…
          </p>
        ) : (
          <Outcome outcome={preview} label="Dry run" />
        )}
        {result && <Outcome outcome={result} label={verb} />}
        {production && !result && (
          <label className="space-y-1 text-xs">
            <span>
              Type <strong className="font-mono">{connection.name}</strong> to confirm
            </span>
            <input
              value={typed}
              onChange={(e) => setTyped(e.target.value)}
              aria-label="Connection name"
              className="h-8 w-full rounded-md border bg-background px-2 font-mono text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring"
            />
          </label>
        )}
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            {result ? "Close" : "Cancel"}
          </Button>
          {!result && (
            <Button
              variant={production ? "destructive" : "default"}
              disabled={
                connection.read_only ||
                preview?.status !== "success" ||
                run.isPending ||
                (production && typed.trim() !== connection.name.trim())
              }
              onClick={execute}
            >
              {run.isPending && preview && <Loader2 className="animate-spin" />} {verb}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
