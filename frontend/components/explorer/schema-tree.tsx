"use client";

import { useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  ChevronRight,
  Copy,
  Database,
  Eye,
  FolderTree,
  KeyRound,
  Link2,
  MoreHorizontal,
  Play,
  RefreshCw,
  Search,
  Table2,
} from "lucide-react";
import { toast } from "sonner";

import { EmptyState } from "@/components/common/empty-state";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { schemaKeys, useColumns, useSchemas, useTables } from "@/hooks/use-schema";
import { formatNumber } from "@/lib/format";
import { qualifiedName } from "@/lib/sql-ident";
import { cn } from "@/lib/utils";
import type { TableInfo } from "@/types/schema";

interface SchemaTreeProps {
  connectionId: string | null;
  databaseName?: string;
  selected?: { schema: string; name: string } | null;
  onSelectTable?: (schema: string, table: string) => void;
  onPreviewTable?: (schema: string, table: string) => void;
}

function TreeRow({
  depth,
  expanded,
  onToggle,
  children,
  className,
  active,
  onClick,
}: {
  depth: number;
  expanded?: boolean;
  onToggle?: () => void;
  children: React.ReactNode;
  className?: string;
  active?: boolean;
  onClick?: () => void;
}) {
  return (
    <div
      role="treeitem"
      aria-expanded={expanded}
      aria-selected={active}
      tabIndex={0}
      onClick={onClick ?? onToggle}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          (onClick ?? onToggle)?.();
        }
        if (e.key === "ArrowRight" && expanded === false) onToggle?.();
        if (e.key === "ArrowLeft" && expanded === true) onToggle?.();
      }}
      className={cn(
        "group flex h-7 cursor-pointer items-center gap-1.5 rounded-md pr-1 text-[13px] outline-none select-none",
        "hover:bg-accent/60 focus-visible:ring-1 focus-visible:ring-ring",
        active && "bg-accent text-accent-foreground",
        className,
      )}
      style={{ paddingLeft: depth * 12 + 4 }}
    >
      {onToggle ? (
        <span
          role="presentation"
          onClick={(e) => {
            if (onClick) {
              e.stopPropagation();
              onToggle();
            }
          }}
          className="flex size-4 shrink-0 items-center justify-center text-muted-foreground"
        >
          <ChevronRight className={cn("size-3.5 transition-transform", expanded && "rotate-90")} />
        </span>
      ) : (
        <span className="size-4 shrink-0" />
      )}
      {children}
    </div>
  );
}

function ColumnsList({ connectionId, table, depth }: { connectionId: string; table: TableInfo; depth: number }) {
  const { data, isPending, isError, error } = useColumns(connectionId, table.schema, table.name);
  if (isPending) {
    return (
      <div className="space-y-1 py-1" style={{ paddingLeft: depth * 12 + 24 }}>
        <Skeleton className="h-4 w-32" />
        <Skeleton className="h-4 w-24" />
      </div>
    );
  }
  if (isError) {
    return (
      <p className="py-1 text-xs text-destructive" style={{ paddingLeft: depth * 12 + 24 }}>
        {error.message}
      </p>
    );
  }
  return (
    <div role="group">
      {data.map((col) => (
        <div
          key={col.name}
          className="flex h-6 items-center gap-1.5 rounded-md pr-2 text-xs hover:bg-accent/40"
          style={{ paddingLeft: depth * 12 + 24 }}
          title={col.comment ?? undefined}
        >
          {col.is_primary_key ? (
            <KeyRound className="size-3 shrink-0 text-warning" aria-label="Primary key" />
          ) : col.foreign_key ? (
            <Link2 className="size-3 shrink-0 text-chart-2" aria-label="Foreign key" />
          ) : (
            <span className="size-3 shrink-0" />
          )}
          <span className="truncate font-mono">{col.name}</span>
          {col.foreign_key && (
            <Tooltip>
              <TooltipTrigger render={<span className="truncate text-[10px] text-chart-2/80" />}>
                → {col.foreign_key.table}
              </TooltipTrigger>
              <TooltipContent side="right">
                References {col.foreign_key.schema}.{col.foreign_key.table}({col.foreign_key.column})
              </TooltipContent>
            </Tooltip>
          )}
          <span className="ml-auto shrink-0 pl-2 font-mono text-[10px] text-muted-foreground">
            {col.data_type}
          </span>
        </div>
      ))}
    </div>
  );
}

function TableNode({
  connectionId,
  table,
  depth,
  active,
  onSelect,
  onPreview,
}: {
  connectionId: string;
  table: TableInfo;
  depth: number;
  active: boolean;
  onSelect?: () => void;
  onPreview?: () => void;
}) {
  const [open, setOpen] = useState(false);
  const isView = table.kind === "view" || table.kind === "materialized_view";
  const Icon = isView ? Eye : Table2;

  return (
    <div>
      <TreeRow
        depth={depth}
        expanded={open}
        onToggle={() => setOpen((o) => !o)}
        onClick={onSelect ? () => {
          onSelect();
          setOpen(true);
        } : undefined}
        active={active}
      >
        <Icon className={cn("size-3.5 shrink-0", isView ? "text-chart-5" : "text-muted-foreground")} />
        <span className="truncate">{table.name}</span>
        {table.estimated_rows !== null && table.estimated_rows >= 0 && (
          <span className="ml-auto shrink-0 text-[10px] text-muted-foreground/70 tabular-nums group-hover:hidden">
            {formatNumber(table.estimated_rows)}
          </span>
        )}
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                variant="ghost"
                size="icon-xs"
                className="ml-auto hidden shrink-0 group-hover:flex aria-expanded:flex"
                aria-label={`Actions for ${table.name}`}
                onClick={(e) => e.stopPropagation()}
              />
            }
          >
            <MoreHorizontal />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-44">
            {onPreview && (
              <DropdownMenuItem onClick={onPreview}>
                <Play /> Preview data
              </DropdownMenuItem>
            )}
            <DropdownMenuItem
              onClick={() => {
                void navigator.clipboard
                  .writeText(qualifiedName(table.schema, table.name))
                  .then(() => toast.success("Copied table name"));
              }}
            >
              <Copy /> Copy name
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </TreeRow>
      {open && <ColumnsList connectionId={connectionId} table={table} depth={depth + 1} />}
    </div>
  );
}

export function SchemaTree({ connectionId, databaseName, selected, onSelectTable, onPreviewTable }: SchemaTreeProps) {
  const queryClient = useQueryClient();
  const schemas = useSchemas(connectionId);
  const tables = useTables(connectionId);
  const [filter, setFilter] = useState("");
  const [expanded, setExpanded] = useState<Record<string, boolean>>({ public: true });

  const grouped = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    const map = new Map<string, TableInfo[]>();
    for (const t of tables.data ?? []) {
      if (needle && !t.name.toLowerCase().includes(needle)) continue;
      const list = map.get(t.schema) ?? [];
      list.push(t);
      map.set(t.schema, list);
    }
    return map;
  }, [tables.data, filter]);

  if (!connectionId) {
    return (
      <EmptyState icon={Database} title="No database selected" description="Choose or add a connection to explore its schema." />
    );
  }

  const loading = schemas.isPending || tables.isPending;
  const error = schemas.error ?? tables.error;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b px-2">
        <Database className="size-3.5 shrink-0 text-brand" />
        <span className="truncate text-xs font-medium">{databaseName ?? "Database"}</span>
        <Button
          variant="ghost"
          size="icon-xs"
          className="ml-auto"
          aria-label="Refresh schema"
          onClick={() => void queryClient.invalidateQueries({ queryKey: schemaKeys.all(connectionId) })}
        >
          <RefreshCw className={cn((schemas.isFetching || tables.isFetching) && "animate-spin")} />
        </Button>
      </div>
      <div className="shrink-0 p-2">
        <div className="relative">
          <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <input
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Search tables…"
            aria-label="Search tables"
            className="h-7 w-full rounded-md border bg-background pl-7 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring"
          />
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-1 pb-2 scrollbar-thin" role="tree" aria-label="Database schema">
        {loading && (
          <div className="space-y-2 p-2">
            {Array.from({ length: 6 }).map((_, i) => (
              <Skeleton key={i} className="h-5" style={{ width: `${60 + ((i * 17) % 35)}%` }} />
            ))}
          </div>
        )}
        {error && <p className="p-3 text-xs text-destructive">{error.message}</p>}
        {!loading &&
          !error &&
          (schemas.data ?? []).map((schema) => {
            const list = grouped.get(schema.name) ?? [];
            if (filter && list.length === 0) return null;
            const isOpen = filter ? true : (expanded[schema.name] ?? false);
            const tablesOnly = list.filter((t) => t.kind !== "view" && t.kind !== "materialized_view");
            const views = list.filter((t) => t.kind === "view" || t.kind === "materialized_view");
            return (
              <div key={schema.name}>
                <TreeRow
                  depth={0}
                  expanded={isOpen}
                  onToggle={() => setExpanded((e) => ({ ...e, [schema.name]: !isOpen }))}
                >
                  <FolderTree className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className="truncate font-medium">{schema.name}</span>
                  <span className="ml-auto text-[10px] text-muted-foreground/70 tabular-nums">{list.length}</span>
                </TreeRow>
                {isOpen && (
                  <div role="group">
                    {list.length === 0 && (
                      <p className="py-1 pl-9 text-xs text-muted-foreground/70">No tables</p>
                    )}
                    {[...tablesOnly, ...views].map((t) => (
                      <TableNode
                        key={`${t.schema}.${t.name}`}
                        connectionId={connectionId}
                        table={t}
                        depth={1}
                        active={selected?.schema === t.schema && selected?.name === t.name}
                        onSelect={onSelectTable ? () => onSelectTable(t.schema, t.name) : undefined}
                        onPreview={onPreviewTable ? () => onPreviewTable(t.schema, t.name) : undefined}
                      />
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        {!loading && !error && schemas.data?.length === 0 && (
          <EmptyState icon={FolderTree} title="No schemas" description="This role cannot see any schemas." />
        )}
      </div>
    </div>
  );
}
