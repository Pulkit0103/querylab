"use client";

import { useState, type ReactNode } from "react";
import Link from "next/link";
import { ArrowRight, Bookmark, Database, History, SquareTerminal } from "lucide-react";

import { PageContainer } from "@/components/common/page-container";
import { ConnectionList } from "@/components/connections/connection-list";
import { ActivityChart, LatencyChart } from "@/components/dashboard/stats-charts";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useStats } from "@/hooks/use-library";
import { formatDuration, formatNumber } from "@/lib/format";

const quickLinks = [
  { href: "/editor", icon: SquareTerminal, title: "Open SQL editor", description: "Write and run queries with autocomplete." },
  { href: "/explorer", icon: Database, title: "Explore schema", description: "Browse schemas, tables, keys and relationships." },
  { href: "/saved", icon: Bookmark, title: "Saved queries", description: "Reuse and organise the queries you rely on." },
  { href: "/history", icon: History, title: "Query history", description: "Review and re-run previous executions." },
];

const RANGES = [
  { value: 7, label: "Last 7 days" },
  { value: 14, label: "Last 14 days" },
  { value: 30, label: "Last 30 days" },
];

function Tile({ label, value, hint }: { label: string; value: ReactNode; hint?: ReactNode }) {
  return (
    <div className="rounded-lg border bg-card px-4 py-3">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="mt-1 text-xl font-semibold tracking-tight tabular-nums">{value}</p>
      {hint && <p className="mt-0.5 text-[11px] text-muted-foreground">{hint}</p>}
    </div>
  );
}

function Panel({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-lg border bg-card">
      <h3 className="border-b px-4 py-3 text-sm font-medium">{title}</h3>
      <div className="p-4">{children}</div>
    </section>
  );
}

export default function DashboardPage() {
  const [days, setDays] = useState(14);
  const stats = useStats(days);
  const t = stats.data?.totals;
  const successRate = t && t.executions > 0 ? `${Math.round((t.successful / t.executions) * 100)}%` : "—";

  return (
    <PageContainer
      title="Dashboard"
      description="Your SQL workspace at a glance."
      actions={
        <Select items={RANGES} value={days} onValueChange={(v) => typeof v === "number" && setDays(v)}>
          <SelectTrigger size="sm" className="w-36" aria-label="Time range">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {RANGES.map((r) => (
              <SelectItem key={r.value} value={r.value}>
                {r.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      }
    >
      <div className="grid gap-3 sm:grid-cols-3 lg:grid-cols-6">
        {stats.isPending || !t ? (
          Array.from({ length: 6 }).map((_, i) => <Skeleton key={i} className="h-20" />)
        ) : (
          <>
            <Tile label="Executions" value={formatNumber(t.executions)} />
            <Tile label="Success rate" value={successRate} hint={`${formatNumber(t.failed)} failed`} />
            <Tile label="Avg duration" value={formatDuration(t.avg_execution_ms)} />
            <Tile label="p95 duration" value={formatDuration(t.p95_execution_ms)} />
            <Tile label="Saved queries" value={formatNumber(t.saved_queries)} />
            <Tile label="Connections" value={formatNumber(t.connections)} />
          </>
        )}
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        <Panel title="Query activity">
          {stats.data ? <ActivityChart daily={stats.data.daily} /> : <Skeleton className="h-[220px]" />}
        </Panel>
        <Panel title="Average execution time">
          {stats.data ? <LatencyChart daily={stats.data.daily} /> : <Skeleton className="h-[220px]" />}
        </Panel>
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        <Panel title="Slowest queries">
          {!stats.data ? (
            <Skeleton className="h-24" />
          ) : stats.data.slowest.length > 0 ? (
            <ul className="divide-y text-xs">
              {stats.data.slowest.map((h) => (
                <li key={h.id} className="flex items-center gap-3 py-2">
                  <code className="min-w-0 flex-1 truncate font-mono">{h.sql_content.replace(/\s+/g, " ")}</code>
                  <span className="shrink-0 text-muted-foreground tabular-nums">{formatDuration(h.execution_time_ms)}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-xs text-muted-foreground">No successful queries in this period.</p>
          )}
        </Panel>
        <Panel title="Most used connections">
          {!stats.data ? (
            <Skeleton className="h-24" />
          ) : stats.data.by_connection.length > 0 ? (
            <ul className="space-y-2 text-xs">
              {stats.data.by_connection.map((c) => {
                const max = stats.data.by_connection[0].executions || 1;
                return (
                  <li key={c.connection_id ?? "deleted"}>
                    <div className="mb-1 flex justify-between">
                      <span>{c.connection_name ?? "Deleted connection"}</span>
                      <span className="text-muted-foreground tabular-nums">{formatNumber(c.executions)}</span>
                    </div>
                    <div className="h-1.5 rounded-full bg-muted">
                      <div className="h-full rounded-full bg-[var(--viz-1)]" style={{ width: `${(c.executions / max) * 100}%` }} />
                    </div>
                  </li>
                );
              })}
            </ul>
          ) : (
            <p className="text-xs text-muted-foreground">No executions in this period.</p>
          )}
        </Panel>
      </div>

      <div className="mt-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {quickLinks.map((l) => (
          <Link
            key={l.href}
            href={l.href}
            className="group rounded-lg border bg-card p-4 transition-colors hover:border-brand/40 hover:bg-accent/30"
          >
            <l.icon className="size-4 text-brand" />
            <p className="mt-3 flex items-center gap-1 text-sm font-medium">
              {l.title}
              <ArrowRight className="size-3.5 opacity-0 transition-all group-hover:translate-x-0.5 group-hover:opacity-100" />
            </p>
            <p className="mt-1 text-xs text-muted-foreground">{l.description}</p>
          </Link>
        ))}
      </div>

      <section className="mt-6 rounded-lg border bg-card">
        <div className="border-b px-4 py-3">
          <h3 className="text-sm font-medium">Connections</h3>
        </div>
        <ConnectionList />
      </section>
    </PageContainer>
  );
}
