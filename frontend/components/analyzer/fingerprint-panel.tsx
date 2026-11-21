"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { FileCode2, Gauge } from "lucide-react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
  type TooltipContentProps,
} from "recharts";

import { EmptyState } from "@/components/common/empty-state";
import { AXIS, ChartTooltip, dayLabel } from "@/components/dashboard/stats-charts";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useConnections } from "@/hooks/use-connections";
import { useFingerprints, useFingerprintTimeline } from "@/hooks/use-performance";
import { formatDuration, formatNumber, formatRelativeTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useEditorStore } from "@/stores/editor-store";
import type { TimelinePoint } from "@/types/performance";

const ALL = "__all__";
const DAYS = [7, 14, 30, 90].map((d) => ({ value: String(d), label: `Last ${d} days` }));

function LatencyTimeline({ points }: { points: TimelinePoint[] }) {
  return (
    <ResponsiveContainer width="100%" height={200}>
      <LineChart data={points} margin={{ top: 8, right: 8, left: -8, bottom: 0 }}>
        <CartesianGrid vertical={false} stroke="var(--border)" />
        <XAxis dataKey="day" tickFormatter={dayLabel} tick={AXIS} axisLine={false} tickLine={false} minTickGap={16} />
        <YAxis tick={AXIS} axisLine={false} tickLine={false} width={64} tickFormatter={(v: number) => (v === 0 ? "0" : formatDuration(v))} />
        <Tooltip
          cursor={{ stroke: "var(--muted-foreground)", strokeDasharray: "3 3" }}
          content={(props) => <ChartTooltip {...(props as TooltipContentProps<number, string>)} unit="ms" />}
        />
        <Legend
          verticalAlign="top"
          align="right"
          height={24}
          iconType="plainline"
          formatter={(v) => <span className="text-xs text-muted-foreground">{v}</span>}
        />
        <Line type="monotone" dataKey="p50_ms" name="Median (p50)" stroke="var(--viz-1)" strokeWidth={2} dot={{ r: 3, strokeWidth: 2, stroke: "var(--card)", fill: "var(--viz-1)" }} connectNulls />
        <Line type="monotone" dataKey="p95_ms" name="p95" stroke="var(--viz-2)" strokeWidth={2} dot={{ r: 3, strokeWidth: 2, stroke: "var(--card)", fill: "var(--viz-2)" }} connectNulls />
      </LineChart>
    </ResponsiveContainer>
  );
}

function ExecutionsTimeline({ points }: { points: TimelinePoint[] }) {
  return (
    <ResponsiveContainer width="100%" height={120}>
      <BarChart data={points} margin={{ top: 8, right: 8, left: -16, bottom: 0 }} barCategoryGap="20%">
        <CartesianGrid vertical={false} stroke="var(--border)" />
        <XAxis dataKey="day" tickFormatter={dayLabel} tick={AXIS} axisLine={false} tickLine={false} minTickGap={16} />
        <YAxis allowDecimals={false} tick={AXIS} axisLine={false} tickLine={false} width={40} />
        <Tooltip
          cursor={{ fill: "var(--accent)", opacity: 0.5 }}
          content={(props) => <ChartTooltip {...(props as TooltipContentProps<number, string>)} unit="count" />}
        />
        <Bar dataKey="executions" name="Executions" fill="var(--viz-1)" radius={[4, 4, 0, 0]} />
      </BarChart>
    </ResponsiveContainer>
  );
}

/** Statements grouped by fingerprint, with a timing timeline per group. */
export function FingerprintPanel() {
  const router = useRouter();
  const newTab = useEditorStore((s) => s.newTab);
  const { data: connections } = useConnections();
  const [connectionId, setConnectionId] = useState<string | null>(null);
  const [days, setDays] = useState(14);
  const [selected, setSelected] = useState<string | null>(null);
  const list = useFingerprints(connectionId, days);
  const current = selected ?? list.data?.[0]?.fingerprint ?? null;
  const timeline = useFingerprintTimeline(current, connectionId, days);

  const connectionItems = [{ value: ALL, label: "All connections" }, ...(connections ?? []).map((c) => ({ value: c.id, label: c.name }))];
  const openSql = (title: string, sql: string) => {
    newTab({ title, sql });
    router.push("/editor");
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-2 border-b px-3 py-2 text-xs">
        <Select
          items={connectionItems}
          value={connectionId ?? ALL}
          onValueChange={(v) => {
            if (typeof v !== "string") return;
            setConnectionId(v === ALL ? null : v);
            setSelected(null);
          }}
        >
          <SelectTrigger size="sm" className="h-7 w-48 text-xs" aria-label="Connection">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {connectionItems.map((c) => (
              <SelectItem key={c.value} value={c.value} className="text-xs">
                {c.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Select items={DAYS} value={String(days)} onValueChange={(v) => typeof v === "string" && setDays(Number(v))}>
          <SelectTrigger size="sm" className="h-7 w-36 text-xs" aria-label="Period">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {DAYS.map((d) => (
              <SelectItem key={d.value} value={d.value} className="text-xs">
                {d.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <span className="text-muted-foreground">
          Queries run from the SQL editor, grouped when they differ only in literal values. Timings use successful runs.
        </span>
      </div>

      {list.isError ? (
        <p className="p-4 text-sm text-destructive">{list.error.message}</p>
      ) : !list.data ? (
        <div className="space-y-2 p-3">
          {Array.from({ length: 6 }).map((_, i) => (
            <Skeleton key={i} className="h-8 w-full" />
          ))}
        </div>
      ) : list.data.length === 0 ? (
        <EmptyState icon={Gauge} title="No queries in this period" description="Run queries in the SQL editor to build a performance history." />
      ) : (
        <div className="grid min-h-0 flex-1 grid-rows-[minmax(0,1fr)_minmax(0,1.3fr)] lg:grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)] lg:grid-rows-1">
          <div className="min-h-0 overflow-auto border-b scrollbar-thin lg:border-r lg:border-b-0">
            <table className="w-full text-left text-xs">
              <thead className="sticky top-0 z-[1] bg-panel text-muted-foreground">
                <tr>
                  <th className="w-full px-3 py-2 font-medium">Statement</th>
                  <th className="px-3 py-2 text-right font-medium">Runs</th>
                  <th className="px-3 py-2 text-right font-medium">p50</th>
                  <th className="px-3 py-2 text-right font-medium">p95</th>
                  <th className="px-3 py-2 text-right font-medium">Total</th>
                </tr>
              </thead>
              <tbody>
                {list.data.map((f) => (
                  <tr
                    key={f.fingerprint}
                    onClick={() => setSelected(f.fingerprint)}
                    data-selected={f.fingerprint === current || undefined}
                    className={cn("cursor-pointer border-t align-top hover:bg-accent/40", f.fingerprint === current && "bg-accent/60")}
                  >
                    <td className="max-w-0 px-3 py-1.5">
                      <span className="line-clamp-2 font-mono break-all" title={f.normalized_sql}>
                        {f.normalized_sql}
                      </span>
                      <span className="text-[11px] text-muted-foreground">
                        {f.connection_name ?? "deleted connection"} · {formatRelativeTime(f.last_executed_at)}
                      </span>
                    </td>
                    <td className="px-3 py-1.5 text-right tabular-nums">
                      {formatNumber(f.executions)}
                      {f.errors > 0 && <span className="block text-[11px] text-destructive">{f.errors} failed</span>}
                    </td>
                    <td className="px-3 py-1.5 text-right whitespace-nowrap tabular-nums">{formatDuration(f.p50_ms)}</td>
                    <td className="px-3 py-1.5 text-right whitespace-nowrap tabular-nums">{formatDuration(f.p95_ms)}</td>
                    <td className="px-3 py-1.5 text-right font-medium whitespace-nowrap tabular-nums">{formatDuration(f.total_ms)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="min-h-0 space-y-4 overflow-auto p-3 scrollbar-thin" aria-label="Query timeline">
            {timeline.isError ? (
              <p className="text-sm text-destructive">{timeline.error.message}</p>
            ) : !timeline.data ? (
              <Skeleton className="h-64 w-full" />
            ) : (
              <>
                <div className="space-y-2">
                  <pre className="max-h-32 overflow-auto rounded-md border bg-panel p-2 font-mono text-[11px] whitespace-pre-wrap">
                    {timeline.data.stats.normalized_sql}
                  </pre>
                  <div className="flex flex-wrap items-center gap-x-5 gap-y-1 text-xs">
                    <span>
                      <span className="text-muted-foreground">Runs </span>
                      <span className="font-medium tabular-nums">{formatNumber(timeline.data.stats.executions)}</span>
                    </span>
                    <span>
                      <span className="text-muted-foreground">Median </span>
                      <span className="font-medium tabular-nums">{formatDuration(timeline.data.stats.p50_ms)}</span>
                    </span>
                    <span>
                      <span className="text-muted-foreground">p95 </span>
                      <span className="font-medium tabular-nums">{formatDuration(timeline.data.stats.p95_ms)}</span>
                    </span>
                    <span>
                      <span className="text-muted-foreground">Slowest </span>
                      <span className="font-medium tabular-nums">{formatDuration(timeline.data.stats.max_ms)}</span>
                    </span>
                    <Button
                      variant="ghost"
                      size="xs"
                      className="ml-auto gap-1"
                      onClick={() => openSql("Analyzer query", timeline.data.stats.sample_sql)}
                    >
                      <FileCode2 className="size-3.5" /> Open latest in editor
                    </Button>
                  </div>
                </div>
                <section>
                  <h3 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">Duration per day</h3>
                  <LatencyTimeline points={timeline.data.points} />
                </section>
                <section>
                  <h3 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">Executions per day</h3>
                  <ExecutionsTimeline points={timeline.data.points} />
                </section>
                <section>
                  <h3 className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">Recent runs</h3>
                  <ul className="divide-y rounded-lg border text-xs">
                    {timeline.data.recent.map((h) => (
                      <li key={h.id} className="flex items-center gap-3 px-3 py-1.5">
                        <span className={cn("w-14 shrink-0", h.status === "success" ? "text-success" : "text-destructive")}>{h.status}</span>
                        <span className="w-20 shrink-0 text-right tabular-nums">{formatDuration(h.execution_time_ms)}</span>
                        <span className="min-w-0 flex-1 truncate font-mono text-muted-foreground" title={h.sql_content}>
                          {h.sql_content}
                        </span>
                        <span className="shrink-0 text-muted-foreground">{formatRelativeTime(h.executed_at)}</span>
                      </li>
                    ))}
                  </ul>
                </section>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
