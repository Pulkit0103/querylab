"use client";

import { useState } from "react";
import { Database, Loader2, Pencil, PlugZap, Plus, ShieldCheck, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { EmptyState } from "@/components/common/empty-state";
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
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  useConnections,
  useConnectionStatus,
  useDeleteConnection,
  useTestSavedConnection,
} from "@/hooks/use-connections";
import { formatDuration, formatRelativeTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { Connection } from "@/types/api";

function StatusDot({ connection }: { connection: Connection }) {
  const { data: status } = useConnectionStatus(connection.id);
  const label = !status
    ? connection.last_connected_at
      ? `Last connected ${formatRelativeTime(connection.last_connected_at)}`
      : "Not tested yet"
    : status.ok
      ? `Connected in ${formatDuration(status.result.latency_ms)}`
      : status.error;
  return (
    <Tooltip>
      <TooltipTrigger render={<span className="flex size-4 items-center justify-center" />}>
        <span
          className={cn(
            "size-2 rounded-full",
            !status && "bg-muted-foreground/40",
            status?.ok === true && "bg-success",
            status?.ok === false && "bg-destructive",
          )}
        />
      </TooltipTrigger>
      <TooltipContent className="max-w-xs">{label}</TooltipContent>
    </Tooltip>
  );
}

function ConnectionRow({ connection, onDelete }: { connection: Connection; onDelete: () => void }) {
  const openDialog = useWorkspaceStore((s) => s.openConnectionDialog);
  const test = useTestSavedConnection();
  return (
    <li className="flex items-center gap-3 px-4 py-3">
      <StatusDot connection={connection} />
      <div className="min-w-0 flex-1">
        <p className="flex items-center gap-2 truncate text-sm font-medium">
          {connection.name}
          {connection.read_only ? (
            <Badge variant="outline" className="gap-1 border-success/30 text-[10px] text-success">
              <ShieldCheck className="size-3" /> read-only
            </Badge>
          ) : (
            <Badge variant="outline" className="border-warning/30 text-[10px] text-warning">
              read-write
            </Badge>
          )}
        </p>
        <p className="truncate font-mono text-xs text-muted-foreground">
          {connection.username}@{connection.host}:{connection.port}/{connection.database} · ssl {connection.ssl_mode}
        </p>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <Button
          size="sm"
          variant="ghost"
          className="gap-1.5"
          disabled={test.isPending}
          onClick={() =>
            test.mutate(connection.id, {
              onSuccess: (r) => toast.success(`${connection.name}: connected in ${formatDuration(r.latency_ms)}`),
              onError: (e) => toast.error(`${connection.name}: ${e.message}`),
            })
          }
        >
          {test.isPending ? <Loader2 className="size-3.5 animate-spin" /> : <PlugZap className="size-3.5" />}
          <span className="hidden sm:inline">Test</span>
        </Button>
        <Button size="icon-sm" variant="ghost" aria-label={`Edit ${connection.name}`} onClick={() => openDialog(connection)}>
          <Pencil className="size-3.5" />
        </Button>
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label={`Delete ${connection.name}`}
          className="hover:text-destructive"
          onClick={onDelete}
        >
          <Trash2 className="size-3.5" />
        </Button>
      </div>
    </li>
  );
}

export function ConnectionList() {
  const { data, isPending, isError, error } = useConnections();
  const openDialog = useWorkspaceStore((s) => s.openConnectionDialog);
  const del = useDeleteConnection();
  const [pendingDelete, setPendingDelete] = useState<Connection | null>(null);

  if (isPending) {
    return (
      <div className="space-y-2 p-4">
        <Skeleton className="h-10" />
        <Skeleton className="h-10" />
      </div>
    );
  }
  if (isError) return <p className="p-4 text-sm text-destructive">{error.message}</p>;
  if (data.length === 0) {
    return (
      <EmptyState
        className="py-10"
        icon={Database}
        title="No connections yet"
        description="Add a PostgreSQL database to start querying."
        action={
          <Button size="sm" onClick={() => openDialog()} className="gap-1.5">
            <Plus className="size-3.5" /> Add connection
          </Button>
        }
      />
    );
  }
  return (
    <>
      <ul className="divide-y">
        {data.map((c) => (
          <ConnectionRow key={c.id} connection={c} onDelete={() => setPendingDelete(c)} />
        ))}
      </ul>
      <AlertDialog open={pendingDelete !== null} onOpenChange={(o) => !o && setPendingDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete “{pendingDelete?.name}”?</AlertDialogTitle>
            <AlertDialogDescription>
              The stored credentials are removed permanently. Saved queries and history are kept but no longer linked
              to this connection.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                if (!pendingDelete) return;
                const target = pendingDelete;
                del.mutate(target.id, {
                  onSuccess: () => toast.success(`Deleted ${target.name}`),
                  onError: (e) => toast.error(e.message),
                });
                setPendingDelete(null);
              }}
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
