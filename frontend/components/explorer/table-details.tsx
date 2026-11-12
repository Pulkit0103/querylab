"use client";

import type { ReactNode } from "react";
import { ArrowLeft, ArrowRight, Eye, KeyRound, Link2, ListTree, Play, Table2 } from "lucide-react";

import { EmptyState } from "@/components/common/empty-state";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useColumns, useIndexes, useRelationships, useTables } from "@/hooks/use-schema";
import { formatBytes, formatNumber } from "@/lib/format";
import type { ApiError } from "@/lib/api-client";

function SectionTitle({ children, count }: { children: ReactNode; count?: number }) {
  return (
    <h3 className="mb-2 flex items-center gap-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">
      {children}
      {count !== undefined && <span className="rounded bg-muted px-1.5 text-[10px] tabular-nums">{count}</span>}
    </h3>
  );
}

function Loading() {
  return (
    <div className="space-y-2">
      {Array.from({ length: 3 }).map((_, i) => (
        <Skeleton key={i} className="h-7 w-full" />
      ))}
    </div>
  );
}

function ErrorText({ error }: { error: ApiError }) {
  return <p className="text-sm text-destructive">{error.message}</p>;
}

export function TableDetails({
  connectionId,
  table,
  onPreview,
  onSelectTable,
}: {
  connectionId: string | null;
  table: { schema: string; name: string } | null;
  onPreview?: (schema: string, table: string) => void;
  onSelectTable?: (schema: string, table: string) => void;
}) {
  const schema = table?.schema ?? "";
  const name = table?.name ?? "";
  const enabled = table !== null;
  const tables = useTables(connectionId);
  const columns = useColumns(connectionId, schema, name, enabled);
  const indexes = useIndexes(connectionId, schema, name, enabled);
  const relationships = useRelationships(connectionId, schema, name, enabled);

  if (!table) {
    return (
      <EmptyState
        icon={Table2}
        title="Select a table"
        description="Choose a table or view in the tree to inspect its columns, keys, indexes and relationships."
      />
    );
  }

  const info = tables.data?.find((t) => t.schema === schema && t.name === name);
  const isView = info?.kind === "view" || info?.kind === "materialized_view";

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-3 border-b px-4 py-3">
        {isView ? <Eye className="size-4 text-chart-5" /> : <Table2 className="size-4 text-brand" />}
        <div className="min-w-0">
          <p className="text-xs text-muted-foreground">{schema}</p>
          <h2 className="truncate font-mono text-sm font-medium">{name}</h2>
        </div>
        {info && (
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Badge variant="outline" className="text-[10px]">
              {info.kind.replace("_", " ")}
            </Badge>
            {info.estimated_rows !== null && <span>~{formatNumber(info.estimated_rows)} rows</span>}
            {info.total_bytes !== null && <span>· {formatBytes(info.total_bytes)}</span>}
          </div>
        )}
        {onPreview && (
          <Button size="sm" variant="outline" className="ml-auto gap-1.5" onClick={() => onPreview(schema, name)}>
            <Play className="size-3" /> Preview data
          </Button>
        )}
      </div>
      <div className="min-h-0 flex-1 space-y-6 overflow-auto p-4 scrollbar-thin">
        {info?.comment && <p className="text-sm text-muted-foreground">{info.comment}</p>}

        <section>
          <SectionTitle count={columns.data?.length}>Columns</SectionTitle>
          {columns.isPending ? (
            <Loading />
          ) : columns.isError ? (
            <ErrorText error={columns.error} />
          ) : (
            <div className="overflow-x-auto rounded-lg border">
              <table className="w-full text-left text-xs">
                <thead className="bg-panel text-muted-foreground">
                  <tr>
                    <th className="px-3 py-2 font-medium">#</th>
                    <th className="px-3 py-2 font-medium">Name</th>
                    <th className="px-3 py-2 font-medium">Type</th>
                    <th className="px-3 py-2 font-medium">Nullable</th>
                    <th className="px-3 py-2 font-medium">Default</th>
                    <th className="px-3 py-2 font-medium">Keys</th>
                  </tr>
                </thead>
                <tbody>
                  {columns.data.map((c) => (
                    <tr key={c.name} className="border-t hover:bg-accent/30" title={c.comment ?? undefined}>
                      <td className="px-3 py-1.5 text-muted-foreground tabular-nums">{c.ordinal}</td>
                      <td className="px-3 py-1.5 font-mono">{c.name}</td>
                      <td className="px-3 py-1.5 font-mono text-muted-foreground">{c.data_type}</td>
                      <td className="px-3 py-1.5">
                        {c.nullable ? "yes" : <span className="text-muted-foreground">no</span>}
                      </td>
                      <td
                        className="max-w-56 truncate px-3 py-1.5 font-mono text-muted-foreground"
                        title={c.default_value ?? undefined}
                      >
                        {c.default_value ?? "—"}
                      </td>
                      <td className="px-3 py-1.5">
                        <div className="flex flex-wrap gap-1">
                          {c.is_primary_key && (
                            <Badge variant="outline" className="gap-1 border-warning/40 text-warning">
                              <KeyRound className="size-3" /> PK
                            </Badge>
                          )}
                          {c.foreign_key && (
                            <button
                              type="button"
                              onClick={() => onSelectTable?.(c.foreign_key!.schema, c.foreign_key!.table)}
                              title={c.foreign_key.constraint}
                            >
                              <Badge variant="outline" className="gap-1 border-chart-2/40 text-chart-2 hover:bg-chart-2/10">
                                <Link2 className="size-3" /> {c.foreign_key.schema}.{c.foreign_key.table}.
                                {c.foreign_key.column}
                              </Badge>
                            </button>
                          )}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>

        {!isView && (
          <section>
            <SectionTitle count={indexes.data?.length}>Indexes</SectionTitle>
            {indexes.isPending ? (
              <Loading />
            ) : indexes.isError ? (
              <ErrorText error={indexes.error} />
            ) : indexes.data.length === 0 ? (
              <p className="text-xs text-muted-foreground">No indexes.</p>
            ) : (
              <ul className="divide-y rounded-lg border">
                {indexes.data.map((ix) => (
                  <li key={ix.name} className="px-3 py-2 text-xs">
                    <p className="flex items-center gap-2 font-mono font-medium">
                      <ListTree className="size-3.5 text-muted-foreground" />
                      {ix.name}
                      {ix.is_primary && <Badge variant="outline" className="text-[10px] text-warning">primary</Badge>}
                      {ix.is_unique && !ix.is_primary && <Badge variant="outline" className="text-[10px]">unique</Badge>}
                    </p>
                    <p className="mt-1 font-mono break-all text-muted-foreground">{ix.definition}</p>
                  </li>
                ))}
              </ul>
            )}
          </section>
        )}

        <section>
          <SectionTitle count={relationships.data?.length}>Relationships</SectionTitle>
          {relationships.isPending ? (
            <Loading />
          ) : relationships.isError ? (
            <ErrorText error={relationships.error} />
          ) : relationships.data.length === 0 ? (
            <p className="text-xs text-muted-foreground">No foreign keys reference or are defined on this table.</p>
          ) : (
            <ul className="divide-y rounded-lg border">
              {relationships.data.map((r) => {
                const outgoing = r.direction === "outgoing";
                const other = outgoing ? { schema: r.to_schema, table: r.to_table } : { schema: r.from_schema, table: r.from_table };
                return (
                  <li key={`${r.direction}-${r.constraint}`} className="flex items-center gap-3 px-3 py-2 text-xs">
                    {outgoing ? (
                      <ArrowRight className="size-3.5 shrink-0 text-chart-2" aria-label="References" />
                    ) : (
                      <ArrowLeft className="size-3.5 shrink-0 text-chart-5" aria-label="Referenced by" />
                    )}
                    <span className="min-w-0 font-mono">
                      <span className="text-muted-foreground">{outgoing ? "references " : "referenced by "}</span>
                      <button
                        type="button"
                        className="text-foreground hover:text-brand hover:underline"
                        onClick={() => onSelectTable?.(other.schema, other.table)}
                      >
                        {other.schema}.{other.table}
                      </button>{" "}
                      <span className="text-muted-foreground">
                        ({r.from_columns.join(", ")} → {r.to_columns.join(", ")})
                      </span>
                    </span>
                    <span className="ml-auto shrink-0 font-mono text-[10px] text-muted-foreground/70">{r.constraint}</span>
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      </div>
    </div>
  );
}
