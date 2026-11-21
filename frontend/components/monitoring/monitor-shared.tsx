"use client";

import { useState } from "react";
import { EyeOff, Info, Loader2, RefreshCw, ShieldAlert } from "lucide-react";
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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useSignalBackend } from "@/hooks/use-monitoring";
import { privilegeNotice, REFRESH_OPTIONS, sessionTone, type SessionTone } from "@/lib/monitoring";
import { cn } from "@/lib/utils";
import type { Connection } from "@/types/api";
import type { BackendSignal, Privileges, Session } from "@/types/monitoring";

const refreshItems = REFRESH_OPTIONS.map((o) => ({ value: String(o.value), label: o.label }));

/** Auto-refresh interval plus a manual refresh button. */
export function RefreshControl({
  interval,
  onIntervalChange,
  onRefresh,
  fetching,
  updatedAt,
}: {
  interval: number;
  onIntervalChange: (ms: number) => void;
  onRefresh: () => void;
  fetching: boolean;
  updatedAt: number;
}) {
  return (
    <div className="flex items-center gap-2 text-xs text-muted-foreground">
      {updatedAt > 0 && <span className="hidden tabular-nums sm:inline">Updated {new Date(updatedAt).toLocaleTimeString()}</span>}
      <span>Refresh</span>
      <Select items={refreshItems} value={String(interval)} onValueChange={(v) => typeof v === "string" && onIntervalChange(Number(v))}>
        <SelectTrigger size="sm" className="h-7 w-20 text-xs" aria-label="Auto-refresh interval">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {refreshItems.map((o) => (
            <SelectItem key={o.value} value={o.value} className="text-xs">
              {o.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Button variant="ghost" size="icon-sm" onClick={onRefresh} aria-label="Refresh now" disabled={fetching}>
        <RefreshCw className={cn("size-3.5", fetching && "animate-spin")} />
      </Button>
    </div>
  );
}

/** Explains what is hidden from a role without monitoring privileges. */
export function PrivilegeNotice({ privileges }: { privileges: Privileges }) {
  const text = privilegeNotice(privileges);
  if (!text) return null;
  return (
    <div className="flex items-start gap-2 rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs" role="note">
      <Info className="mt-0.5 size-3.5 shrink-0 text-warning" />
      <span>{text}</span>
    </div>
  );
}

const TONE: Record<SessionTone, { label: (s: Session) => string; className: string }> = {
  active: { label: () => "active", className: "bg-success/15 text-success" },
  waiting: { label: () => "waiting", className: "bg-destructive/15 text-destructive" },
  idle: { label: () => "idle", className: "bg-muted text-muted-foreground" },
  "idle-in-transaction": { label: (s) => s.state ?? "idle in transaction", className: "bg-warning/15 text-warning" },
  other: { label: (s) => s.state ?? s.backend_type ?? "unknown", className: "bg-muted text-muted-foreground" },
};

export function SessionStateBadge({ session }: { session: Session }) {
  const tone = TONE[sessionTone(session)];
  return (
    <span className={cn("rounded px-1.5 py-0.5 text-[10px] font-medium whitespace-nowrap", tone.className)}>
      {tone.label(session)}
    </span>
  );
}

/** Statement text, or an explanation when PostgreSQL hides it. */
export function QueryText({ session, className }: { session: Session; className?: string }) {
  if (session.query_hidden) {
    return (
      <span className={cn("flex items-center gap-1 text-muted-foreground italic", className)}>
        <EyeOff className="size-3" /> hidden (insufficient privilege)
      </span>
    );
  }
  if (!session.query) return <span className={cn("text-muted-foreground", className)}>—</span>;
  return (
    <span className={cn("font-mono break-all", className)} title={session.query}>
      {session.query}
    </span>
  );
}

const SIGNAL_TEXT: Record<BackendSignal, { title: string; body: string; action: string }> = {
  cancel: {
    title: "Cancel the running statement",
    body: "The session stays connected; its current statement fails with “canceling statement due to user request”.",
    action: "Cancel statement",
  },
  terminate: {
    title: "Terminate the session",
    body: "The connection is closed and any open transaction is rolled back.",
    action: "Terminate session",
  },
};

/** Confirmation (typed for production) before cancel or terminate. */
export function SignalDialog({
  connection,
  target,
  onClose,
}: {
  connection: Pick<Connection, "id" | "name" | "environment">;
  target: { session: Session; signal: BackendSignal } | null;
  onClose: () => void;
}) {
  const [typed, setTyped] = useState("");
  const signal = useSignalBackend(connection.id);
  const production = connection.environment === "production";
  const text = target ? SIGNAL_TEXT[target.signal] : null;

  const run = () => {
    if (!target) return;
    signal.mutate(
      {
        pid: target.session.pid,
        signal: target.signal,
        confirm: connection.environment === "development" ? undefined : typed || "confirm",
      },
      {
        onSuccess: () => toast.success(`Session ${target.session.pid}: ${target.signal === "cancel" ? "statement cancelled" : "terminated"}`),
        onError: (e) => toast.error(e.message),
        onSettled: onClose,
      },
    );
  };

  return (
    <AlertDialog
      open={target !== null}
      onOpenChange={(open) => {
        if (!open) onClose();
        setTyped("");
      }}
    >
      <AlertDialogContent className="sm:max-w-md">
        <AlertDialogHeader>
          <AlertDialogTitle className="flex items-center gap-2">
            <ShieldAlert className={cn("size-4", production ? "text-destructive" : "text-warning")} />
            {text?.title} {target && <span className="font-mono">#{target.session.pid}</span>}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {text?.body} Session of <strong>{target?.session.user ?? "unknown role"}</strong> on{" "}
            <strong>{connection.name}</strong> ({connection.environment}). The action is recorded in your history.
          </AlertDialogDescription>
        </AlertDialogHeader>
        {target?.session.query && (
          <pre className="max-h-24 overflow-auto rounded bg-muted p-2 font-mono text-[11px] whitespace-pre-wrap">
            {target.session.query}
          </pre>
        )}
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
          <AlertDialogCancel>Keep running</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={signal.isPending || (production && typed.trim() !== connection.name.trim())}
            onClick={run}
          >
            {signal.isPending && <Loader2 className="animate-spin" />} {text?.action}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
