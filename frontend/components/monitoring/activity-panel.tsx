"use client";

import { useMemo, useState } from "react";
import { MoreHorizontal, Search } from "lucide-react";

import {
  PrivilegeNotice,
  QueryText,
  RefreshControl,
  SessionStateBadge,
  SignalDialog,
} from "@/components/monitoring/monitor-shared";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { useActivity } from "@/hooks/use-monitoring";
import { formatDuration } from "@/lib/format";
import { defaultSessionFilter, filterSessions, LONG_TRANSACTION_MS, signalBlocker } from "@/lib/monitoring";
import { cn } from "@/lib/utils";
import type { Connection } from "@/types/api";
import type { BackendSignal, Session } from "@/types/monitoring";

export function ActivityPanel({
  connection,
  interval,
  onIntervalChange,
}: {
  connection: Pick<Connection, "id" | "name" | "environment">;
  interval: number;
  onIntervalChange: (ms: number) => void;
}) {
  const activity = useActivity(connection.id, interval || false);
  const [filter, setFilter] = useState(defaultSessionFilter);
  const [target, setTarget] = useState<{ session: Session; signal: BackendSignal } | null>(null);
  const data = activity.data;
  const sessions = useMemo(() => (data ? filterSessions(data.sessions, filter) : []), [data, filter]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-3 border-b px-3 py-2">
        <div className="relative">
          <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={filter.search}
            onChange={(e) => setFilter((f) => ({ ...f, search: e.target.value }))}
            placeholder="Filter sessions…"
            aria-label="Filter sessions"
            className="h-7 w-52 pl-7 text-xs"
          />
        </div>
        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Switch checked={filter.activeOnly} onCheckedChange={(v) => setFilter((f) => ({ ...f, activeOnly: v }))} />
          Hide idle
        </label>
        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Switch checked={filter.clientsOnly} onCheckedChange={(v) => setFilter((f) => ({ ...f, clientsOnly: v }))} />
          Client sessions only
        </label>
        {data && (
          <span className="text-xs text-muted-foreground tabular-nums">
            {sessions.length} of {data.sessions.length} sessions
          </span>
        )}
        <div className="ml-auto">
          <RefreshControl
            interval={interval}
            onIntervalChange={onIntervalChange}
            onRefresh={() => void activity.refetch()}
            fetching={activity.isFetching}
            updatedAt={activity.dataUpdatedAt}
          />
        </div>
      </div>
      {data && (
        <div className="shrink-0 px-3 pt-2 empty:hidden">
          <PrivilegeNotice privileges={data.privileges} />
        </div>
      )}
      {activity.isError && !data ? (
        <p className="p-4 text-sm text-destructive">{activity.error.message}</p>
      ) : !data ? (
        <div className="space-y-2 p-3">
          {Array.from({ length: 6 }).map((_, i) => (
            <Skeleton key={i} className="h-8 w-full" />
          ))}
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-auto scrollbar-thin">
          {activity.isError && <p className="px-3 pt-2 text-xs text-destructive">{activity.error.message}</p>}
          <table className="w-full text-left text-xs">
            <thead className="sticky top-0 z-[1] bg-panel text-muted-foreground">
              <tr>
                <th className="px-3 py-2 font-medium">PID</th>
                <th className="px-3 py-2 font-medium">State</th>
                <th className="px-3 py-2 font-medium">User · application</th>
                <th className="px-3 py-2 text-right font-medium">Running</th>
                <th className="px-3 py-2 font-medium">Waiting on</th>
                <th className="w-full px-3 py-2 font-medium">Statement</th>
                <th className="px-3 py-2">
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {sessions.map((s) => {
                const blocker = signalBlocker(data.privileges, s);
                const longTx = (s.xact_ms ?? 0) > LONG_TRANSACTION_MS;
                return (
                  <tr key={s.pid} className="border-t align-top hover:bg-accent/30" data-pid={s.pid}>
                    <td className="px-3 py-1.5 font-mono tabular-nums">{s.pid}</td>
                    <td className="px-3 py-1.5">
                      <SessionStateBadge session={s} />
                    </td>
                    <td className="px-3 py-1.5 whitespace-nowrap">
                      <span>{s.user ?? "—"}</span>
                      {(s.application || s.client) && (
                        <span className="block text-[11px] text-muted-foreground">
                          {[s.application, s.client].filter(Boolean).join(" · ")}
                        </span>
                      )}
                    </td>
                    <td className="px-3 py-1.5 text-right whitespace-nowrap tabular-nums">
                      {s.state === "active" ? formatDuration(s.query_ms) : "—"}
                      {s.xact_ms !== null && (
                        <span className={cn("block text-[11px]", longTx ? "text-warning" : "text-muted-foreground")}>
                          tx {formatDuration(s.xact_ms)}
                        </span>
                      )}
                    </td>
                    <td className="px-3 py-1.5 whitespace-nowrap">
                      {s.blocked_by.length > 0 ? (
                        <span className="text-destructive">pid {s.blocked_by.join(", ")}</span>
                      ) : s.wait_event ? (
                        <span className="text-muted-foreground">
                          {s.wait_event_type}: {s.wait_event}
                        </span>
                      ) : (
                        <span className="text-muted-foreground">—</span>
                      )}
                    </td>
                    <td className="max-w-0 px-3 py-1.5">
                      <QueryText session={s} className="line-clamp-2" />
                    </td>
                    <td className="px-2 py-1">
                      <DropdownMenu>
                        <DropdownMenuTrigger
                          render={<Button variant="ghost" size="icon-xs" aria-label={`Actions for session ${s.pid}`} />}
                        >
                          <MoreHorizontal className="size-3.5" />
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end" className="w-56">
                          <DropdownMenuItem
                            disabled={blocker !== null || s.state !== "active"}
                            onClick={() => setTarget({ session: s, signal: "cancel" })}
                          >
                            Cancel statement
                          </DropdownMenuItem>
                          <DropdownMenuItem
                            variant="destructive"
                            disabled={blocker !== null}
                            onClick={() => setTarget({ session: s, signal: "terminate" })}
                          >
                            Terminate session
                          </DropdownMenuItem>
                          {blocker && <p className="px-2 py-1.5 text-[11px] text-muted-foreground">{blocker}</p>}
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {sessions.length === 0 && <p className="p-4 text-center text-xs text-muted-foreground">No sessions match.</p>}
        </div>
      )}
      <SignalDialog connection={connection} target={target} onClose={() => setTarget(null)} />
    </div>
  );
}
