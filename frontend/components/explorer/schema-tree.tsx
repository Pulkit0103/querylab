"use client";

import { useMemo, useState, type ReactNode } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  Braces,
  ChevronRight,
  Columns3,
  Copy,
  Database,
  Eye,
  FileCode2,
  FolderTree,
  FunctionSquare,
  Hash,
  KeyRound,
  Layers,
  Link2,
  ListOrdered,
  MoreHorizontal,
  Play,
  PlusSquare,
  RefreshCw,
  Search,
  Shapes,
  Table2,
  TextSelect,
} from "lucide-react";
import { toast } from "sonner";

import { EmptyState } from "@/components/common/empty-state";
import { Button } from "@/components/ui/button";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  columnsQuery,
  ddlQuery,
  functionsQuery,
  schemaKeys,
  useColumns,
  useFunctions,
  useSchemaObjects,
  useSchemaSearch,
  useSchemas,
  useTables,
} from "@/hooks/use-schema";
import { ApiError } from "@/lib/api-client";
import { extrasBySchema, functionSignature, sameObject, type ExplorerObject } from "@/lib/explorer";
import { formatNumber } from "@/lib/format";
import { insertTemplate, qualifiedName, selectSql } from "@/lib/sql-ident";
import { cn } from "@/lib/utils";
import type { SearchHit, TableInfo } from "@/types/schema";

interface SchemaTreeProps {
  connectionId: string | null;
  databaseName?: string;
  selected?: { schema: string; name: string } | null;
  selectedObject?: ExplorerObject | null;
  onSelectTable?: (schema: string, table: string) => void;
  onSelectObject?: (object: ExplorerObject) => void;
  onPreviewTable?: (schema: string, table: string) => void;
  /** Opens generated SQL (SELECT, INSERT, DDL) in a new editor tab. */
  onOpenSql?: (title: string, sql: string) => void;
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
  children: ReactNode;
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
          <span className="ml-auto shrink-0 pl-2 font-mono text-[10px] text-muted-foreground">{col.data_type}</span>
        </div>
      ))}
    </div>
  );
}

/** Menu entries shared by the "⋯" dropdown and the right-click menu. */
function TableMenuItems({
  connectionId,
  table,
  onPreview,
  onOpenSql,
}: {
  connectionId: string;
  table: TableInfo;
  onPreview?: () => void;
  onOpenSql?: (title: string, sql: string) => void;
}) {
  const qc = useQueryClient();
  const qualified = qualifiedName(table.schema, table.name);
  const fail = (e: unknown) => toast.error(e instanceof ApiError ? e.message : "Could not load table metadata.");

  const withColumns = (build: (cols: Awaited<ReturnType<ReturnType<typeof columnsQuery>["queryFn"]>>) => void) =>
    qc.fetchQuery(columnsQuery(connectionId, table.schema, table.name)).then(build, fail);

  const copy = (text: string, label: string) =>
    void navigator.clipboard.writeText(text).then(
      () => toast.success(`Copied ${label}`),
      () => toast.error("Clipboard access was denied by the browser."),
    );

  return (
    <>
      {onPreview && (
        <DropdownMenuItem onClick={onPreview}>
          <Play /> View records
        </DropdownMenuItem>
      )}
      {onOpenSql && (
        <>
          <DropdownMenuItem
            onClick={() =>
              void withColumns((cols) =>
                onOpenSql(
                  table.name,
                  selectSql(
                    table.schema,
                    table.name,
                    cols.map((c) => c.name),
                  ),
                ),
              )
            }
          >
            <TextSelect /> Generate SELECT
          </DropdownMenuItem>
          {table.kind !== "view" && table.kind !== "materialized_view" && (
            <DropdownMenuItem
              onClick={() =>
                void withColumns((cols) => onOpenSql(`insert ${table.name}`, insertTemplate(table.schema, table.name, cols)))
              }
            >
              <PlusSquare /> Generate INSERT
            </DropdownMenuItem>
          )}
          <DropdownMenuItem
            onClick={() =>
              void qc
                .fetchQuery(ddlQuery(connectionId, table.schema, table.name))
                .then((d) => onOpenSql(`${table.name}.sql`, d.ddl), fail)
            }
          >
            <FileCode2 /> {table.kind === "view" || table.kind === "materialized_view" ? "Generate CREATE VIEW" : "Generate CREATE TABLE"}
          </DropdownMenuItem>
        </>
      )}
      <DropdownMenuSeparator />
      <DropdownMenuItem onClick={() => copy(table.name, "table name")}>
        <Copy /> Copy name
      </DropdownMenuItem>
      <DropdownMenuItem onClick={() => copy(qualified, "qualified name")}>
        <Copy /> Copy qualified name
      </DropdownMenuItem>
    </>
  );
}

function TableNode({
  connectionId,
  table,
  depth,
  active,
  onSelect,
  onPreview,
  onOpenSql,
}: {
  connectionId: string;
  table: TableInfo;
  depth: number;
  active: boolean;
  onSelect?: () => void;
  onPreview?: () => void;
  onOpenSql?: (title: string, sql: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const isView = table.kind === "view" || table.kind === "materialized_view";
  const Icon = isView ? Eye : Table2;
  const items = <TableMenuItems connectionId={connectionId} table={table} onPreview={onPreview} onOpenSql={onOpenSql} />;

  return (
    <div>
      <ContextMenu>
        <ContextMenuTrigger render={<div />}>
          <TreeRow
            depth={depth}
            expanded={open}
            onToggle={() => setOpen((o) => !o)}
            onClick={
              onSelect
                ? () => {
                    onSelect();
                    setOpen(true);
                  }
                : undefined
            }
            active={active}
          >
            <Icon className={cn("size-3.5 shrink-0", isView ? "text-chart-5" : "text-muted-foreground")} />
            <span className="truncate">{table.name}</span>
            {table.kind === "materialized_view" && (
              <span className="shrink-0 text-[9px] text-muted-foreground uppercase">mat</span>
            )}
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
              <DropdownMenuContent align="start" className="w-52">
                {items}
              </DropdownMenuContent>
            </DropdownMenu>
          </TreeRow>
        </ContextMenuTrigger>
        <ContextMenuContent className="w-52" aria-label={`${table.name} menu`}>
          {items}
        </ContextMenuContent>
      </ContextMenu>
      {open && <ColumnsList connectionId={connectionId} table={table} depth={depth + 1} />}
    </div>
  );
}

function Folder({
  depth,
  icon: Icon,
  label,
  count,
  open,
  onToggle,
  children,
}: {
  depth: number;
  icon: typeof Layers;
  label: string;
  count?: number;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <div>
      <TreeRow depth={depth} expanded={open} onToggle={onToggle} className="text-muted-foreground">
        <Icon className="size-3.5 shrink-0" />
        <span className="truncate text-xs">{label}</span>
        {count !== undefined && <span className="ml-auto text-[10px] text-muted-foreground/70 tabular-nums">{count}</span>}
      </TreeRow>
      {open && <div role="group">{children}</div>}
    </div>
  );
}

function LeafRow({
  depth,
  icon: Icon,
  label,
  detail,
  active,
  onClick,
  title,
}: {
  depth: number;
  icon: typeof Layers;
  label: string;
  detail?: string;
  active?: boolean;
  onClick: () => void;
  title?: string;
}) {
  return (
    <TreeRow depth={depth} active={active} onClick={onClick}>
      <Icon className="size-3.5 shrink-0 text-muted-foreground" />
      <span className="truncate font-mono text-xs" title={title}>
        {label}
      </span>
      {detail && <span className="ml-auto shrink-0 pl-2 text-[10px] text-muted-foreground">{detail}</span>}
    </TreeRow>
  );
}

const HIT_ICON: Record<string, typeof Layers> = { column: Columns3, function: FunctionSquare };

function SearchResults({
  hits,
  onPick,
}: {
  hits: SearchHit[];
  onPick: (hit: SearchHit) => void;
}) {
  const others = hits.filter((h) => h.kind === "column" || h.kind === "function");
  if (others.length === 0) return null;
  return (
    <div className="mb-1 border-b pb-1" role="group" aria-label="Column and function matches">
      <p className="px-2 py-1 text-[10px] font-medium tracking-wide text-muted-foreground uppercase">
        Columns &amp; functions
      </p>
      {others.slice(0, 25).map((h) => {
        const Icon = HIT_ICON[h.kind] ?? Hash;
        return (
          <button
            key={`${h.kind}:${h.schema}.${h.table ?? ""}.${h.name}:${h.detail ?? ""}`}
            type="button"
            onClick={() => onPick(h)}
            className="flex h-7 w-full items-center gap-1.5 rounded-md px-2 text-left text-xs hover:bg-accent/60"
          >
            <Icon className="size-3.5 shrink-0 text-muted-foreground" />
            <span className="truncate font-mono">
              {h.kind === "column" ? `${h.table}.${h.name}` : `${h.name}(${h.detail ?? ""})`}
            </span>
            <span className="ml-auto shrink-0 pl-2 text-[10px] text-muted-foreground">
              {h.kind === "column" ? h.detail : h.schema}
            </span>
          </button>
        );
      })}
    </div>
  );
}

export function SchemaTree({
  connectionId,
  databaseName,
  selected,
  selectedObject = null,
  onSelectTable,
  onSelectObject,
  onPreviewTable,
  onOpenSql,
}: SchemaTreeProps) {
  const queryClient = useQueryClient();
  const schemas = useSchemas(connectionId);
  const tables = useTables(connectionId);
  const [filter, setFilter] = useState("");
  const [expanded, setExpanded] = useState<Record<string, boolean>>({ public: true });
  // Functions and objects load the first time any such folder is opened.
  const wantFunctions = Object.entries(expanded).some(([k, v]) => v && k.endsWith(":functions"));
  const wantObjects = Object.entries(expanded).some(([k, v]) => v && (k.endsWith(":sequences") || k.endsWith(":types")));
  const functions = useFunctions(connectionId, wantFunctions);
  const objects = useSchemaObjects(connectionId, wantObjects);
  const search = useSchemaSearch(onSelectObject || onSelectTable ? connectionId : null, filter);

  const toggle = (key: string, current: boolean) => setExpanded((e) => ({ ...e, [key]: !current }));

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
  const extras = useMemo(() => extrasBySchema(functions.data, objects.data), [functions.data, objects.data]);

  if (!connectionId) {
    return (
      <EmptyState icon={Database} title="No database selected" description="Choose or add a connection to explore its schema." />
    );
  }

  const loading = schemas.isPending || tables.isPending;
  const error = schemas.error ?? tables.error;
  const filtering = filter.trim() !== "";

  const pickHit = (hit: SearchHit) => {
    if (hit.kind === "column" && hit.table) onSelectTable?.(hit.schema, hit.table);
    else if (hit.kind === "function") {
      // Functions load lazily; fetch (or reuse) the list, then select the match.
      void queryClient.fetchQuery(functionsQuery(connectionId)).then(
        (list) => {
          const fn = list.find((f) => f.schema === hit.schema && f.name === hit.name && f.arguments === hit.detail);
          if (fn) onSelectObject?.({ kind: "function", oid: fn.oid, schema: fn.schema, name: fn.name });
          setExpanded((e) => ({ ...e, [hit.schema]: true, [`${hit.schema}:functions`]: true }));
        },
        (e: unknown) => toast.error(e instanceof ApiError ? e.message : "Could not load functions."),
      );
    }
  };

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
            placeholder="Search tables, columns, functions…"
            aria-label="Search schema"
            className="h-7 w-full rounded-md border bg-background pl-7 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring"
          />
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-1 pb-2 scrollbar-thin" role="tree" aria-label="Database schema">
        {filtering && search.data && <SearchResults hits={search.data} onPick={pickHit} />}
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
            if (filtering && list.length === 0) return null;
            const isOpen = filtering ? true : (expanded[schema.name] ?? false);
            const tablesOnly = list.filter((t) => t.kind !== "view" && t.kind !== "materialized_view");
            const views = list.filter((t) => t.kind === "view" || t.kind === "materialized_view");
            const ex = extras.get(schema.name);
            const key = (part: string) => `${schema.name}:${part}`;
            const folderOpen = (part: string, fallback = false) => expanded[key(part)] ?? fallback;
            const tableNode = (t: TableInfo) => (
              <TableNode
                key={`${t.schema}.${t.name}`}
                connectionId={connectionId}
                table={t}
                depth={2}
                active={!selectedObject && selected?.schema === t.schema && selected?.name === t.name}
                onSelect={onSelectTable ? () => onSelectTable(t.schema, t.name) : undefined}
                onPreview={onPreviewTable ? () => onPreviewTable(t.schema, t.name) : undefined}
                onOpenSql={onOpenSql}
              />
            );
            return (
              <div key={schema.name}>
                <TreeRow depth={0} expanded={isOpen} onToggle={() => toggle(schema.name, isOpen)}>
                  <FolderTree className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className="truncate font-medium">{schema.name}</span>
                  <span className="ml-auto text-[10px] text-muted-foreground/70 tabular-nums">{list.length}</span>
                </TreeRow>
                {isOpen && (
                  <div role="group">
                    <Folder
                      depth={1}
                      icon={Table2}
                      label="Tables"
                      count={tablesOnly.length}
                      open={filtering || folderOpen("tables", true)}
                      onToggle={() => toggle(key("tables"), folderOpen("tables", true))}
                    >
                      {tablesOnly.length === 0 ? (
                        <p className="py-1 pl-12 text-xs text-muted-foreground/70">No tables</p>
                      ) : (
                        tablesOnly.map(tableNode)
                      )}
                    </Folder>
                    {views.length > 0 && (
                      <Folder
                        depth={1}
                        icon={Eye}
                        label="Views"
                        count={views.length}
                        open={filtering || folderOpen("views", true)}
                        onToggle={() => toggle(key("views"), folderOpen("views", true))}
                      >
                        {views.map(tableNode)}
                      </Folder>
                    )}
                    {!filtering && onSelectObject && (
                      <>
                        <Folder
                          depth={1}
                          icon={FunctionSquare}
                          label="Functions"
                          count={functions.data ? (ex?.functions.length ?? 0) : undefined}
                          open={folderOpen("functions")}
                          onToggle={() => toggle(key("functions"), folderOpen("functions"))}
                        >
                          {functions.isPending ? (
                            <Skeleton className="my-1 ml-12 h-4 w-32" />
                          ) : functions.isError ? (
                            <p className="py-1 pl-12 text-xs text-destructive">{functions.error.message}</p>
                          ) : (ex?.functions.length ?? 0) === 0 ? (
                            <p className="py-1 pl-12 text-xs text-muted-foreground/70">No functions</p>
                          ) : (
                            ex!.functions.map((f) => {
                              const obj = { kind: "function", oid: f.oid, schema: f.schema, name: f.name } as const;
                              return (
                                <LeafRow
                                  key={f.oid}
                                  depth={2}
                                  icon={f.kind === "procedure" ? Braces : FunctionSquare}
                                  label={functionSignature(f)}
                                  title={functionSignature(f)}
                                  detail={f.kind === "procedure" ? "proc" : (f.result ?? undefined)}
                                  active={sameObject(selectedObject, obj)}
                                  onClick={() => onSelectObject(obj)}
                                />
                              );
                            })
                          )}
                        </Folder>
                        <Folder
                          depth={1}
                          icon={ListOrdered}
                          label="Sequences"
                          count={objects.data ? (ex?.sequences.length ?? 0) : undefined}
                          open={folderOpen("sequences")}
                          onToggle={() => toggle(key("sequences"), folderOpen("sequences"))}
                        >
                          {objects.isPending ? (
                            <Skeleton className="my-1 ml-12 h-4 w-32" />
                          ) : (ex?.sequences.length ?? 0) === 0 ? (
                            <p className="py-1 pl-12 text-xs text-muted-foreground/70">No sequences</p>
                          ) : (
                            ex!.sequences.map((s) => {
                              const obj = { kind: "sequence", schema: s.schema, name: s.name } as const;
                              return (
                                <LeafRow
                                  key={s.name}
                                  depth={2}
                                  icon={ListOrdered}
                                  label={s.name}
                                  detail={s.data_type}
                                  active={sameObject(selectedObject, obj)}
                                  onClick={() => onSelectObject(obj)}
                                />
                              );
                            })
                          )}
                        </Folder>
                        <Folder
                          depth={1}
                          icon={Shapes}
                          label="Types"
                          count={objects.data ? (ex?.types.length ?? 0) : undefined}
                          open={folderOpen("types")}
                          onToggle={() => toggle(key("types"), folderOpen("types"))}
                        >
                          {objects.isPending ? (
                            <Skeleton className="my-1 ml-12 h-4 w-32" />
                          ) : objects.isError ? (
                            <p className="py-1 pl-12 text-xs text-destructive">{objects.error.message}</p>
                          ) : (ex?.types.length ?? 0) === 0 ? (
                            <p className="py-1 pl-12 text-xs text-muted-foreground/70">No types</p>
                          ) : (
                            ex!.types.map((t) => {
                              const obj = { kind: "type", schema: t.schema, name: t.name } as const;
                              return (
                                <LeafRow
                                  key={t.name}
                                  depth={2}
                                  icon={Shapes}
                                  label={t.name}
                                  detail={t.kind}
                                  active={sameObject(selectedObject, obj)}
                                  onClick={() => onSelectObject(obj)}
                                />
                              );
                            })
                          )}
                        </Folder>
                      </>
                    )}
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
