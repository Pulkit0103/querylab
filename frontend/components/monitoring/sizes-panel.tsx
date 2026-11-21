"use client";

import { useState } from "react";
import { Database, RefreshCw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useSizes } from "@/hooks/use-monitoring";
import { formatBytes, formatNumber, formatRelativeTime } from "@/lib/format";
import { deadRatio, share } from "@/lib/monitoring";
import { cn } from "@/lib/utils";

const ALL = "__all__";

function Bar({ value, title }: { value: number; title: string }) {
  return (
    <span className="block h-1.5 w-full overflow-hidden rounded-full bg-muted" title={title}>
      <span className="block h-full rounded-full" style={{ width: `${Math.max(value * 100, value > 0 ? 1 : 0)}%`, background: "var(--viz-1)" }} />
    </span>
  );
}

/** Table vs index bytes as one stacked bar (legend above the table). */
function SplitBar({ table, index, max }: { table: number; index: number; max: number }) {
  return (
    <span className="flex h-1.5 w-full gap-px overflow-hidden rounded-full bg-muted">
      <span style={{ width: `${share(table, max) * 100}%`, background: "var(--viz-1)" }} />
      <span style={{ width: `${share(index, max) * 100}%`, background: "var(--viz-2)" }} />
    </span>
  );
}

export function SizesPanel({ connectionId }: { connectionId: string }) {
  const [schema, setSchema] = useState<string | null>(null);
  const sizes = useSizes(connectionId, schema);
  const data = sizes.data;
  const schemaItems = [{ value: ALL, label: "All schemas" }, ...(data?.schemas ?? []).map((s) => ({ value: s.schema, label: s.schema }))];
  const maxTable = Math.max(1, ...(data?.tables ?? []).map((t) => t.total_bytes));

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-3 border-b px-3 py-2 text-xs">
        <Select items={schemaItems} value={schema ?? ALL} onValueChange={(v) => typeof v === "string" && setSchema(v === ALL ? null : v)}>
          <SelectTrigger size="sm" className="h-7 w-44 text-xs" aria-label="Schema">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {schemaItems.map((s) => (
              <SelectItem key={s.value} value={s.value} className="text-xs">
                {s.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <span className="text-muted-foreground">Sizes are read on demand.</span>
        <Button variant="ghost" size="sm" className="ml-auto h-7 gap-1.5 text-xs" onClick={() => void sizes.refetch()} disabled={sizes.isFetching}>
          <RefreshCw className={cn("size-3.5", sizes.isFetching && "animate-spin")} /> Refresh
        </Button>
      </div>
      {sizes.isError && !data ? (
        <p className="p-4 text-sm text-destructive">{sizes.error.message}</p>
      ) : !data ? (
        <div className="space-y-2 p-3">
          {Array.from({ length: 6 }).map((_, i) => (
            <Skeleton key={i} className="h-8 w-full" />
          ))}
        </div>
      ) : (
        <div className="min-h-0 flex-1 space-y-6 overflow-auto p-3 scrollbar-thin">
          <div className="flex flex-wrap gap-6">
            <div className="flex items-center gap-3">
              <Database className="size-5 text-brand" />
              <div>
                <p className="text-xs text-muted-foreground">Database {data.database}</p>
                <p className="text-2xl font-semibold tabular-nums">{formatBytes(data.database_bytes)}</p>
              </div>
            </div>
            <ul className="min-w-64 flex-1 space-y-1.5 text-xs" aria-label="Schema sizes">
              {data.schemas.slice(0, 8).map((s) => (
                <li key={s.schema} className="grid grid-cols-[8rem_1fr_5rem] items-center gap-3">
                  <span className="truncate font-mono">{s.schema}</span>
                  <Bar value={share(s.total_bytes, data.database_bytes)} title={`${Math.round(share(s.total_bytes, data.database_bytes) * 100)}% of the database`} />
                  <span className="text-right tabular-nums">{formatBytes(s.total_bytes)}</span>
                </li>
              ))}
            </ul>
          </div>

          <section aria-label="Table sizes">
            <div className="mb-2 flex items-center gap-4 text-xs">
              <h3 className="font-medium tracking-wide text-muted-foreground uppercase">Largest tables</h3>
              <span className="flex items-center gap-1.5 text-muted-foreground">
                <span className="size-2 rounded-sm" style={{ background: "var(--viz-1)" }} /> Table (incl. TOAST)
              </span>
              <span className="flex items-center gap-1.5 text-muted-foreground">
                <span className="size-2 rounded-sm" style={{ background: "var(--viz-2)" }} /> Indexes
              </span>
            </div>
            <div className="overflow-x-auto rounded-lg border">
              <table className="w-full text-left text-xs">
                <thead className="bg-panel text-muted-foreground">
                  <tr>
                    <th className="px-3 py-2 font-medium">Table</th>
                    <th className="w-48 px-3 py-2 font-medium">Size</th>
                    <th className="px-3 py-2 text-right font-medium">Total</th>
                    <th className="px-3 py-2 text-right font-medium">Table</th>
                    <th className="px-3 py-2 text-right font-medium">Indexes</th>
                    <th className="px-3 py-2 text-right font-medium">Rows (est.)</th>
                    <th className="px-3 py-2 text-right font-medium">Dead</th>
                    <th className="px-3 py-2 font-medium">Last vacuum</th>
                  </tr>
                </thead>
                <tbody>
                  {data.tables.map((t) => {
                    const dead = deadRatio(t.live_rows, t.dead_rows);
                    return (
                      <tr key={`${t.schema}.${t.name}`} className="border-t hover:bg-accent/30">
                        <td className="px-3 py-1.5 font-mono whitespace-nowrap">
                          <span className="text-muted-foreground">{t.schema}.</span>
                          {t.name}
                        </td>
                        <td className="px-3 py-1.5">
                          <SplitBar table={t.table_bytes} index={t.index_bytes} max={maxTable} />
                        </td>
                        <td className="px-3 py-1.5 text-right font-medium tabular-nums">{formatBytes(t.total_bytes)}</td>
                        <td className="px-3 py-1.5 text-right tabular-nums">{formatBytes(t.table_bytes)}</td>
                        <td className="px-3 py-1.5 text-right tabular-nums">{formatBytes(t.index_bytes)}</td>
                        <td className="px-3 py-1.5 text-right tabular-nums">{formatNumber(t.estimated_rows)}</td>
                        <td className={cn("px-3 py-1.5 text-right tabular-nums", dead !== null && dead > 0.2 && "text-warning")}>
                          {dead === null ? "—" : `${Math.round(dead * 100)}%`}
                        </td>
                        <td className="px-3 py-1.5 whitespace-nowrap text-muted-foreground">
                          {t.last_vacuum ? formatRelativeTime(t.last_vacuum) : "never"}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </section>

          <section aria-label="Index sizes">
            <h3 className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">Largest indexes</h3>
            <div className="overflow-x-auto rounded-lg border">
              <table className="w-full text-left text-xs">
                <thead className="bg-panel text-muted-foreground">
                  <tr>
                    <th className="px-3 py-2 font-medium">Index</th>
                    <th className="px-3 py-2 font-medium">Table</th>
                    <th className="px-3 py-2 text-right font-medium">Size</th>
                    <th className="px-3 py-2 text-right font-medium">Scans</th>
                  </tr>
                </thead>
                <tbody>
                  {data.indexes.map((i) => {
                    const unused = i.scans === 0 && !i.primary && !i.unique;
                    return (
                      <tr key={`${i.schema}.${i.name}`} className="border-t hover:bg-accent/30">
                        <td className="px-3 py-1.5 font-mono">
                          {i.name}
                          {i.primary && <span className="ml-2 text-[10px] text-warning">primary</span>}
                          {i.unique && !i.primary && <span className="ml-2 text-[10px] text-muted-foreground">unique</span>}
                        </td>
                        <td className="px-3 py-1.5 font-mono text-muted-foreground">
                          {i.schema}.{i.table}
                        </td>
                        <td className="px-3 py-1.5 text-right tabular-nums">{formatBytes(i.bytes)}</td>
                        <td className={cn("px-3 py-1.5 text-right tabular-nums", unused && "text-warning")} title={unused ? "Never used since statistics were reset" : undefined}>
                          {formatNumber(i.scans)}
                          {unused && " · unused"}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
