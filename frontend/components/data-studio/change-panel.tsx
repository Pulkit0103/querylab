"use client";

import { useState } from "react";
import { AlertTriangle, ChevronDown, ChevronUp, Loader2, ShieldAlert } from "lucide-react";
import { toast } from "sonner";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { useApplyChanges, usePreview } from "@/hooks/use-data-studio";
import { changeCount, changeTarget, toChangeSet, type StagedChanges } from "@/lib/data-studio";
import { cn } from "@/lib/utils";
import type { Connection } from "@/types/api";
import type { ApplyResponse } from "@/types/data";

interface ChangePanelProps {
  connection: Pick<Connection, "id" | "name" | "environment">;
  schema: string;
  table: string;
  changes: StagedChanges;
  onDiscard: () => void;
  onApplied: (result: ApplyResponse) => void;
  onFailure: (target: { rowId?: string; insertId?: string }) => void;
}

const KIND_LABEL = { insert: "INSERT", update: "UPDATE", delete: "DELETE" } as const;

/** Pending changes, their exact SQL, and Apply / Discard. */
export function ChangePanel({ connection, schema, table, changes, onDiscard, onApplied, onFailure }: ChangePanelProps) {
  const [showSql, setShowSql] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [typed, setTyped] = useState("");
  const [failure, setFailure] = useState<ApplyResponse["failure"]>(null);
  const changeSet = toChangeSet(changes);
  const preview = usePreview(showSql ? connection.id : null, showSql ? { schema, table, changes: changeSet } : null);
  const apply = useApplyChanges(connection.id);

  const counts = {
    inserts: changes.inserts.length,
    updates: Object.keys(changes.edits).length,
    deletes: Object.keys(changes.deletes).length,
  };
  const total = changeCount(changes);
  const production = connection.environment === "production";

  const run = () => {
    setConfirming(false);
    setFailure(null);
    apply.mutate(
      { schema, table, changes: changeSet, confirm: connection.environment === "development" ? undefined : typed || "confirm" },
      {
        onSuccess: (result) => {
          if (result.committed) {
            toast.success(`Applied ${result.rows_affected} change${result.rows_affected === 1 ? "" : "s"}`);
            onApplied(result);
          } else if (result.failure) {
            setFailure(result.failure);
            onFailure(changeTarget(changes, result.failure.kind, result.failure.index));
          }
        },
        onError: (e) => toast.error(e.message),
      },
    );
  };

  const requestApply = () => {
    setTyped("");
    if (connection.environment === "development") run();
    else setConfirming(true);
  };

  return (
    <div className="border-t bg-panel" aria-label="Pending changes">
      <div className="flex flex-wrap items-center gap-2 px-3 py-2 text-xs">
        <span className="font-medium">
          {total} pending change{total === 1 ? "" : "s"}
        </span>
        <span className="text-muted-foreground">
          {counts.inserts > 0 && <span className="text-success">+{counts.inserts} insert </span>}
          {counts.updates > 0 && <span className="text-warning">~{counts.updates} update </span>}
          {counts.deletes > 0 && <span className="text-destructive">−{counts.deletes} delete</span>}
        </span>
        <Button variant="ghost" size="xs" className="ml-auto" onClick={() => setShowSql((s) => !s)}>
          {showSql ? <ChevronDown /> : <ChevronUp />} Review SQL
        </Button>
        <Button variant="ghost" size="xs" onClick={onDiscard} disabled={apply.isPending}>
          Discard
        </Button>
        <Button size="xs" onClick={requestApply} disabled={apply.isPending}>
          {apply.isPending && <Loader2 className="animate-spin" />} Apply changes
        </Button>
      </div>

      {failure && (
        <div role="alert" className="mx-3 mb-2 flex gap-2 rounded-md border border-destructive/40 bg-destructive/10 p-2 text-xs">
          <AlertTriangle className="size-4 shrink-0 text-destructive" />
          <div className="min-w-0 space-y-1">
            <p className="font-medium">Nothing was applied: the transaction was rolled back.</p>
            <p>{failure.message}</p>
            {failure.sql_error?.detail && <p className="text-muted-foreground">{failure.sql_error.detail}</p>}
            <pre className="truncate font-mono text-[11px] text-muted-foreground" title={failure.statement}>
              {failure.statement}
            </pre>
          </div>
        </div>
      )}

      {showSql && (
        <div className="max-h-48 overflow-auto border-t px-3 py-2 scrollbar-thin" aria-label="Generated statements">
          {preview.error ? (
            <p className="text-xs text-destructive">{preview.error.message}</p>
          ) : !preview.data ? (
            <p className="text-xs text-muted-foreground">Rendering SQL…</p>
          ) : (
            <ol className="space-y-1 font-mono text-[11px]">
              {preview.data.statements.map((s) => (
                <li key={`${s.kind}-${s.index}`} className="flex gap-2">
                  <span
                    className={cn(
                      "w-12 shrink-0 font-sans text-[10px] font-medium",
                      s.kind === "insert" ? "text-success" : s.kind === "update" ? "text-warning" : "text-destructive",
                    )}
                  >
                    {KIND_LABEL[s.kind]}
                  </span>
                  <span className="break-all whitespace-pre-wrap">{s.sql}</span>
                </li>
              ))}
            </ol>
          )}
          <p className="mt-2 text-[11px] text-muted-foreground">
            Statements run in one transaction. Each update and delete must match exactly one row, and edited cells must
            still hold the values you loaded; otherwise nothing is applied.
          </p>
        </div>
      )}

      <AlertDialog open={confirming} onOpenChange={setConfirming}>
        <AlertDialogContent className="sm:max-w-md">
          <AlertDialogHeader>
            <AlertDialogTitle className="flex items-center gap-2">
              <ShieldAlert className={cn("size-4", production ? "text-destructive" : "text-warning")} />
              Apply {total} change{total === 1 ? "" : "s"} to {connection.environment}?
            </AlertDialogTitle>
            <AlertDialogDescription>
              {schema}.{table} on <strong>{connection.name}</strong> will be modified in one transaction.
            </AlertDialogDescription>
          </AlertDialogHeader>
          {production && (
            <label className="space-y-1 text-xs">
              <span>
                Type <strong className="font-mono">{connection.name}</strong> to confirm
              </span>
              <input
                autoFocus
                value={typed}
                onChange={(e) => setTyped(e.target.value)}
                aria-label="Connection name"
                className="h-8 w-full rounded-md border bg-background px-2 font-mono text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring"
              />
            </label>
          )}
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant={production ? "destructive" : "default"}
              disabled={production && typed.trim() !== connection.name.trim()}
              onClick={run}
            >
              Apply changes
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
