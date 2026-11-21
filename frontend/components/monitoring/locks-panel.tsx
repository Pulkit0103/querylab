"use client";

import { useState } from "react";
import { CheckCircle2, CornerDownRight, Lock } from "lucide-react";

import {
  PrivilegeNotice,
  QueryText,
  RefreshControl,
  SessionStateBadge,
  SignalDialog,
} from "@/components/monitoring/monitor-shared";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useLocks } from "@/hooks/use-monitoring";
import { formatDuration } from "@/lib/format";
import { flattenBlocking, signalBlocker } from "@/lib/monitoring";
import { cn } from "@/lib/utils";
import type { Connection } from "@/types/api";
import type { BackendSignal, Session } from "@/types/monitoring";

export function LocksPanel({
  connection,
  interval,
  onIntervalChange,
}: {
  connection: Pick<Connection, "id" | "name" | "environment">;
  interval: number;
  onIntervalChange: (ms: number) => void;
}) {
  const locks = useLocks(connection.id, interval || false);
  const [target, setTarget] = useState<{ session: Session; signal: BackendSignal } | null>(null);
  const data = locks.data;
  const rows = data ? flattenBlocking(data.tree) : [];

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-3 border-b px-3 py-2 text-xs">
        {data && (
          <span className={cn("font-medium", data.waiting > 0 ? "text-destructive" : "text-muted-foreground")}>
            {data.waiting === 0 ? "No session is waiting for a lock" : `${data.waiting} session${data.waiting === 1 ? "" : "s"} waiting`}
          </span>
        )}
        <div className="ml-auto">
          <RefreshControl
            interval={interval}
            onIntervalChange={onIntervalChange}
            onRefresh={() => void locks.refetch()}
            fetching={locks.isFetching}
            updatedAt={locks.dataUpdatedAt}
          />
        </div>
      </div>
      {locks.isError && !data ? (
        <p className="p-4 text-sm text-destructive">{locks.error.message}</p>
      ) : !data ? (
        <div className="space-y-2 p-3">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-8 w-full" />
          ))}
        </div>
      ) : (
        <div className="min-h-0 flex-1 space-y-4 overflow-auto p-3 scrollbar-thin">
          <PrivilegeNotice privileges={data.privileges} />
          {rows.length === 0 ? (
            <div className="flex flex-col items-center gap-2 py-10 text-center text-sm text-muted-foreground">
              <CheckCircle2 className="size-6 text-success" />
              No blocking: every lock request has been granted.
            </div>
          ) : (
            <section aria-label="Blocking tree">
              <h3 className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">Blocking tree</h3>
              <ul className="divide-y rounded-lg border">
                {rows.map(({ node, depth }, i) => {
                  const s = data.sessions[String(node.pid)];
                  const blocker = s ? signalBlocker(data.privileges, s) : "Session details are not available.";
                  return (
                    <li
                      key={`${node.pid}-${i}`}
                      className="flex items-start gap-3 px-3 py-2 text-xs"
                      style={{ paddingLeft: 12 + depth * 24 }}
                      data-pid={node.pid}
                      data-depth={depth}
                    >
                      {depth > 0 ? (
                        <CornerDownRight className="mt-0.5 size-3.5 shrink-0 text-destructive" aria-label="waits for the session above" />
                      ) : (
                        <Lock className="mt-0.5 size-3.5 shrink-0 text-warning" aria-label="blocking" />
                      )}
                      <div className="min-w-0 flex-1 space-y-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="font-mono font-medium">pid {node.pid}</span>
                          {s && <SessionStateBadge session={s} />}
                          {s?.user && <span className="text-muted-foreground">{s.user}</span>}
                          {s?.xact_ms != null && (
                            <span className="text-muted-foreground tabular-nums">transaction {formatDuration(s.xact_ms)}</span>
                          )}
                          {depth === 0 && (
                            <span className="text-muted-foreground">
                              blocks {node.children.length} session{node.children.length === 1 ? "" : "s"}
                            </span>
                          )}
                        </div>
                        {s && <QueryText session={s} className="block" />}
                      </div>
                      {s && (
                        <div className="flex shrink-0 gap-1">
                          {s.state === "active" && (
                            <Button
                              variant="ghost"
                              size="xs"
                              disabled={blocker !== null}
                              title={blocker ?? undefined}
                              onClick={() => setTarget({ session: s, signal: "cancel" })}
                            >
                              Cancel
                            </Button>
                          )}
                          <Button
                            variant="ghost"
                            size="xs"
                            className="text-destructive"
                            disabled={blocker !== null}
                            title={blocker ?? undefined}
                            onClick={() => setTarget({ session: s, signal: "terminate" })}
                          >
                            Terminate
                          </Button>
                        </div>
                      )}
                    </li>
                  );
                })}
              </ul>
            </section>
          )}

          {data.locks.length > 0 && (
            <section aria-label="Locks">
              <h3 className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">
                Locks held and requested by these sessions
              </h3>
              <div className="overflow-x-auto rounded-lg border">
                <table className="w-full text-left text-xs">
                  <thead className="bg-panel text-muted-foreground">
                    <tr>
                      <th className="px-3 py-2 font-medium">PID</th>
                      <th className="px-3 py-2 font-medium">Lock</th>
                      <th className="px-3 py-2 font-medium">Mode</th>
                      <th className="px-3 py-2 font-medium">Relation</th>
                      <th className="px-3 py-2 font-medium">Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.locks.map((l, i) => (
                      <tr key={i} className="border-t">
                        <td className="px-3 py-1.5 font-mono tabular-nums">{l.pid}</td>
                        <td className="px-3 py-1.5">{l.locktype}</td>
                        <td className="px-3 py-1.5 font-mono">{l.mode}</td>
                        <td className="px-3 py-1.5 font-mono">{l.relation ?? "—"}</td>
                        <td className={cn("px-3 py-1.5", l.granted ? "text-muted-foreground" : "font-medium text-destructive")}>
                          {l.granted ? "held" : "waiting"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </section>
          )}
        </div>
      )}
      <SignalDialog connection={connection} target={target} onClose={() => setTarget(null)} />
    </div>
  );
}
