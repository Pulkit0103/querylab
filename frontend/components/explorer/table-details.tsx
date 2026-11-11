"use client";

import { KeyRound, Link2, Play, Table2 } from "lucide-react";

import { EmptyState } from "@/components/common/empty-state";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useColumns } from "@/hooks/use-schema";

export function TableDetails({
  connectionId,
  table,
  onPreview,
}: {
  connectionId: string | null;
  table: { schema: string; name: string } | null;
  onPreview?: (schema: string, table: string) => void;
}) {
  const columns = useColumns(connectionId, table?.schema ?? "", table?.name ?? "", table !== null);

  if (!table) {
    return (
      <EmptyState
        icon={Table2}
        title="Select a table"
        description="Choose a table or view in the tree to inspect its columns, keys and relationships."
      />
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-3 border-b px-4 py-3">
        <Table2 className="size-4 text-brand" />
        <div className="min-w-0">
          <p className="text-xs text-muted-foreground">{table.schema}</p>
          <h2 className="truncate font-mono text-sm font-medium">{table.name}</h2>
        </div>
        {onPreview && (
          <Button size="sm" variant="outline" className="ml-auto gap-1.5" onClick={() => onPreview(table.schema, table.name)}>
            <Play className="size-3" /> Preview data
          </Button>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-4 scrollbar-thin">
        <h3 className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">Columns</h3>
        {columns.isPending ? (
          <div className="space-y-2">
            {Array.from({ length: 5 }).map((_, i) => (
              <Skeleton key={i} className="h-7 w-full" />
            ))}
          </div>
        ) : columns.isError ? (
          <p className="text-sm text-destructive">{columns.error.message}</p>
        ) : (
          <div className="overflow-hidden rounded-lg border">
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
                  <tr key={c.name} className="border-t hover:bg-accent/30">
                    <td className="px-3 py-1.5 text-muted-foreground tabular-nums">{c.ordinal}</td>
                    <td className="px-3 py-1.5 font-mono">{c.name}</td>
                    <td className="px-3 py-1.5 font-mono text-muted-foreground">{c.data_type}</td>
                    <td className="px-3 py-1.5">{c.nullable ? "yes" : <span className="text-muted-foreground">no</span>}</td>
                    <td className="max-w-56 truncate px-3 py-1.5 font-mono text-muted-foreground" title={c.default_value ?? undefined}>
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
                          <Badge variant="outline" className="gap-1 border-chart-2/40 text-chart-2" title={c.foreign_key.constraint}>
                            <Link2 className="size-3" /> {c.foreign_key.schema}.{c.foreign_key.table}.{c.foreign_key.column}
                          </Badge>
                        )}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
