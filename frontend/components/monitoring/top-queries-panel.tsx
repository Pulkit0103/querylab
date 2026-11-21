"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { FileCode2, Info } from "lucide-react";

import { PrivilegeNotice, RefreshControl } from "@/components/monitoring/monitor-shared";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useTopQueries } from "@/hooks/use-monitoring";
import { formatDuration, formatNumber } from "@/lib/format";
import { share } from "@/lib/monitoring";
import { useEditorStore } from "@/stores/editor-store";
import type { TopQueriesSort } from "@/types/monitoring";

const SORTS: { value: TopQueriesSort; label: string }[] = [
  { value: "total", label: "Total time" },
  { value: "mean", label: "Mean time" },
  { value: "calls", label: "Calls" },
];

export function TopQueriesPanel({
  connectionId,
  interval,
  onIntervalChange,
}: {
  connectionId: string;
  interval: number;
  onIntervalChange: (ms: number) => void;
}) {
  const router = useRouter();
  const newTab = useEditorStore((s) => s.newTab);
  const [sort, setSort] = useState<TopQueriesSort>("total");
  const top = useTopQueries(connectionId, sort, interval || false);
  const data = top.data;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-3 border-b px-3 py-2 text-xs">
        <span className="text-muted-foreground">Sort by</span>
        <Select items={SORTS} value={sort} onValueChange={(v) => v && setSort(v as TopQueriesSort)}>
          <SelectTrigger size="sm" className="h-7 w-32 text-xs" aria-label="Sort by">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {SORTS.map((s) => (
              <SelectItem key={s.value} value={s.value} className="text-xs">
                {s.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <span className="text-muted-foreground">From pg_stat_statements, for this database since the last reset.</span>
        <div className="ml-auto">
          <RefreshControl
            interval={interval}
            onIntervalChange={onIntervalChange}
            onRefresh={() => void top.refetch()}
            fetching={top.isFetching}
            updatedAt={top.dataUpdatedAt}
          />
        </div>
      </div>
      {top.isError && !data ? (
        <p className="p-4 text-sm text-destructive">{top.error.message}</p>
      ) : !data ? (
        <div className="space-y-2 p-3">
          {Array.from({ length: 6 }).map((_, i) => (
            <Skeleton key={i} className="h-8 w-full" />
          ))}
        </div>
      ) : !data.available ? (
        <div className="m-4 flex max-w-2xl gap-3 rounded-lg border bg-panel p-4 text-sm" role="note">
          <Info className="mt-0.5 size-4 shrink-0 text-brand" />
          <div className="space-y-1">
            <p className="font-medium">Statement statistics are not available</p>
            <p className="text-muted-foreground">{data.reason}</p>
            <p className="text-xs text-muted-foreground">
              The Analyzer still shows timings of queries run from QueryLab.
            </p>
          </div>
        </div>
      ) : (
        <div className="min-h-0 flex-1 space-y-2 overflow-auto p-3 scrollbar-thin">
          <PrivilegeNotice privileges={data.privileges} />
          <div className="overflow-x-auto rounded-lg border">
            <table className="w-full text-left text-xs">
              <thead className="bg-panel text-muted-foreground">
                <tr>
                  <th className="w-full px-3 py-2 font-medium">Statement</th>
                  <th className="px-3 py-2 text-right font-medium">Calls</th>
                  <th className="px-3 py-2 text-right font-medium">Total</th>
                  <th className="px-3 py-2 text-right font-medium">Mean</th>
                  <th className="px-3 py-2 text-right font-medium">Max</th>
                  <th className="px-3 py-2 text-right font-medium">Rows</th>
                  <th className="px-3 py-2 text-right font-medium" title="Shared buffer hits as a share of all block accesses">
                    Cache hit
                  </th>
                  <th className="px-3 py-2">
                    <span className="sr-only">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {data.queries.map((q, i) => {
                  const hit = share(q.shared_hit, q.shared_hit + q.shared_read);
                  return (
                    <tr key={q.query_id ?? i} className="border-t align-top hover:bg-accent/30">
                      <td className="max-w-0 px-3 py-1.5">
                        {q.query ? (
                          <span className="line-clamp-2 font-mono break-all" title={q.query}>
                            {q.query}
                          </span>
                        ) : (
                          <span className="text-muted-foreground italic">hidden (insufficient privilege)</span>
                        )}
                      </td>
                      <td className="px-3 py-1.5 text-right tabular-nums">{formatNumber(q.calls)}</td>
                      <td className="px-3 py-1.5 text-right font-medium whitespace-nowrap tabular-nums">{formatDuration(q.total_ms)}</td>
                      <td className="px-3 py-1.5 text-right whitespace-nowrap tabular-nums">{formatDuration(q.mean_ms)}</td>
                      <td className="px-3 py-1.5 text-right whitespace-nowrap tabular-nums">{formatDuration(q.max_ms)}</td>
                      <td className="px-3 py-1.5 text-right tabular-nums">{formatNumber(q.rows)}</td>
                      <td className="px-3 py-1.5 text-right tabular-nums">
                        {q.shared_hit + q.shared_read === 0 ? "—" : `${Math.round(hit * 100)}%`}
                      </td>
                      <td className="px-2 py-1">
                        {q.query && (
                          <Button
                            variant="ghost"
                            size="icon-xs"
                            aria-label="Open in editor"
                            title="Open in editor"
                            onClick={() => {
                              newTab({ title: "Top query", sql: q.query! });
                              router.push("/editor");
                            }}
                          >
                            <FileCode2 className="size-3.5" />
                          </Button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          {data.queries.length === 0 && <p className="p-4 text-center text-xs text-muted-foreground">No statements recorded yet.</p>}
        </div>
      )}
    </div>
  );
}
