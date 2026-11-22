"use client";

import { useMemo, useState } from "react";
import { Download, ScrollText } from "lucide-react";

import { EmptyState } from "@/components/common/empty-state";
import { PageContainer } from "@/components/common/page-container";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useAuditEvents } from "@/hooks/use-account";
import { useConnections } from "@/hooks/use-connections";
import { ACTION_GROUPS, actionLabel, exportQuery, isFailure, summarize } from "@/lib/audit";
import { cn } from "@/lib/utils";
import type { AuditFilter } from "@/types/account";

const ALL = "__all__";

/** Local calendar day → ISO bounds. */
function dayStart(date: string): string | undefined {
  return date ? new Date(`${date}T00:00:00`).toISOString() : undefined;
}
function dayEnd(date: string): string | undefined {
  if (!date) return undefined;
  const d = new Date(`${date}T00:00:00`);
  d.setDate(d.getDate() + 1);
  return d.toISOString();
}

export default function AuditPage() {
  const { data: connections } = useConnections();
  const [action, setAction] = useState("");
  const [connectionId, setConnectionId] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const filter: AuditFilter = useMemo(
    () => ({
      action: action || undefined,
      connection_id: connectionId || undefined,
      from: dayStart(from),
      to: dayEnd(to),
    }),
    [action, connectionId, from, to],
  );
  const events = useAuditEvents(filter);
  const rows = events.data?.pages.flatMap((p) => p.events) ?? [];
  const dateInput =
    "h-8 rounded-md border bg-background px-2 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring";

  return (
    <PageContainer
      title="Audit Logs"
      description="Sign-ins, account changes, executed queries and every privileged action. Events cannot be edited or deleted."
      actions={
        <Button
          size="sm"
          variant="outline"
          render={<a href={`/api/audit-events/export${exportQuery(filter as Record<string, string>)}`} download />}
        >
          <Download className="size-3.5" /> Export CSV
        </Button>
      }
    >
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <Select
          items={ACTION_GROUPS.map((g) => ({ value: g.value || ALL, label: g.label }))}
          value={action || ALL}
          onValueChange={(v) => typeof v === "string" && setAction(v === ALL ? "" : v)}
        >
          <SelectTrigger size="sm" className="h-8 w-52 text-xs" aria-label="Action">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {ACTION_GROUPS.map((g) => (
              <SelectItem key={g.value || ALL} value={g.value || ALL} className="text-xs">
                {g.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select
          items={[{ value: ALL, label: "All connections" }, ...(connections ?? []).map((c) => ({ value: c.id, label: c.name }))]}
          value={connectionId || ALL}
          onValueChange={(v) => typeof v === "string" && setConnectionId(v === ALL ? "" : v)}
        >
          <SelectTrigger size="sm" className="h-8 w-52 text-xs" aria-label="Connection">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={ALL} className="text-xs">
              All connections
            </SelectItem>
            {(connections ?? []).map((c) => (
              <SelectItem key={c.id} value={c.id} className="text-xs">
                {c.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
          From
          <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} className={dateInput} aria-label="From date" />
        </label>
        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
          To
          <input type="date" value={to} onChange={(e) => setTo(e.target.value)} className={dateInput} aria-label="To date" />
        </label>
      </div>

      {events.isPending ? (
        <Skeleton className="h-64" />
      ) : events.isError ? (
        <p className="text-sm text-destructive">{events.error.message}</p>
      ) : rows.length === 0 ? (
        <EmptyState icon={ScrollText} title="No events" description="Nothing matches these filters." className="h-64" />
      ) : (
        <div className="overflow-auto rounded-lg border bg-card">
          <table className="w-full text-xs">
            <thead className="bg-panel text-left text-muted-foreground">
              <tr>
                <th className="px-3 py-2 font-medium">Time</th>
                <th className="px-3 py-2 font-medium">Actor</th>
                <th className="px-3 py-2 font-medium">Action</th>
                <th className="px-3 py-2 font-medium">Connection</th>
                <th className="px-3 py-2 font-medium">Details</th>
                <th className="px-3 py-2 font-medium">Address</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((e) => (
                <tr key={e.id} className="border-t align-top">
                  <td className="px-3 py-1.5 whitespace-nowrap text-muted-foreground" title={e.request_id ?? undefined}>
                    {new Date(e.created_at).toLocaleString()}
                  </td>
                  <td className="max-w-40 truncate px-3 py-1.5" title={e.actor_email ?? undefined}>{e.actor_email ?? "—"}</td>
                  <td className={cn("px-3 py-1.5 whitespace-nowrap", isFailure(e) && "text-destructive")} title={e.action}>
                    {actionLabel(e.action)}
                  </td>
                  <td className="px-3 py-1.5 whitespace-nowrap">{e.connection_name ?? (e.connection_id ? "deleted" : "—")}</td>
                  <td className="max-w-sm truncate px-3 py-1.5 font-mono text-[11px]" title={e.detail ? JSON.stringify(e.detail, null, 2) : undefined}>
                    {summarize(e)}
                  </td>
                  <td className="px-3 py-1.5 whitespace-nowrap text-muted-foreground">{e.ip ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {events.hasNextPage && (
        <Button variant="outline" size="sm" className="mt-3" onClick={() => void events.fetchNextPage()} disabled={events.isFetchingNextPage}>
          Load more
        </Button>
      )}
    </PageContainer>
  );
}
