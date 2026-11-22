"use client";

import { useEffect, useMemo, useState } from "react";
import {
  ChevronLeft,
  ChevronRight,
  Copy,
  Download,
  Filter,
  Lock,
  Plus,
  RefreshCw,
  Search,
  X,
} from "lucide-react";
import { toast } from "sonner";

import { ChangePanel } from "@/components/data-studio/change-panel";
import { ColumnVisibility } from "@/components/data-studio/column-visibility";
import { DataGrid, type GridColumn } from "@/components/data-studio/data-grid";
import { FilterBuilder } from "@/components/data-studio/filter-builder";
import { TablePicker } from "@/components/data-studio/table-picker";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { downloadExport, useBrowse, usePreview } from "@/hooks/use-data-studio";
import { EXPORT_FORMATS } from "@/lib/download";
import { ApiError } from "@/lib/api-client";
import {
  addInsert,
  changeCount,
  countFilterConditions,
  emptyChanges,
  loadHidden,
  newCondition,
  newGroup,
  rowRef,
  rowsToJson,
  rowsToTsv,
  saveHidden,
  toApiFilter,
  visibilityKey,
  type UiGroup,
} from "@/lib/data-studio";
import { formatNumber } from "@/lib/format";
import { studioTableKey, useDataStudioStore } from "@/stores/data-studio-store";
import type { Connection } from "@/types/api";
import type { BrowseRequest, ExportFormat, FilterGroup, SortKey, ValueMap } from "@/types/data";

const PAGE_SIZES = [50, 100, 250, 500, 1000];
const NO_SELECTION: ReadonlySet<string> = new Set();

function useDebounced<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(id);
  }, [value, ms]);
  return debounced;
}

interface TableEditorProps {
  connection: Connection;
  schema: string;
  table: string;
  onPickTable: (schema: string, table: string) => void;
}

/** Browse, filter, edit and export the rows of one table. */
export function TableEditor({ connection, schema, table, onPickTable }: TableEditorProps) {
  const [draft, setDraft] = useState<UiGroup>(() => newGroup("and", [newCondition()]));
  const [filter, setFilter] = useState<FilterGroup | null>(null);
  const [filterOpen, setFilterOpen] = useState(false);
  const [sort, setSort] = useState<SortKey[]>([]);
  const [searchInput, setSearchInput] = useState("");
  const search = useDebounced(searchInput.trim(), 400);
  const [pageSize, setPageSize] = useState(100);

  // Paging and selection belong to one view (filter, sort, search, page
  // size): when the view changes they start again from the first page.
  const viewKey = JSON.stringify([filter, sort, search, pageSize]);
  const [paging, setPaging] = useState<{ key: string; cursors: (string | null)[] }>({ key: viewKey, cursors: [null] });
  const cursors = paging.key === viewKey ? paging.cursors : [null];
  const setCursors = (next: (string | null)[]) => setPaging({ key: viewKey, cursors: next });
  const page = cursors.length - 1;
  const pageKey = `${viewKey}#${page}`;
  const [selection, setSelection] = useState<{ key: string; ids: Set<string> }>({ key: "", ids: new Set() });
  const selected = selection.key === pageKey ? selection.ids : NO_SELECTION;
  const setSelected = (ids: Set<string>) => setSelection({ key: pageKey, ids });
  const [failure, setFailure] = useState<{ rowId?: string; insertId?: string }>({});

  const hiddenKey = visibilityKey(connection.id, schema, table);
  const [hidden, setHidden] = useState<string[]>(() => loadHidden(hiddenKey));

  const tableKey = studioTableKey(connection.id, schema, table);
  const changes = useDataStudioStore((s) => s.changes[tableKey]) ?? emptyChanges();
  const updateChanges = useDataStudioStore((s) => s.update);
  const clearChanges = useDataStudioStore((s) => s.clear);
  const onChanges = (fn: Parameters<typeof updateChanges>[1]) => {
    setFailure({});
    updateChanges(tableKey, fn);
  };

  const request: BrowseRequest = {
    schema,
    table,
    filter,
    sort,
    search: search || undefined,
    page_size: pageSize,
    cursor: cursors[page],
  };
  const browse = useBrowse(connection.id, request);
  const data = browse.data?.schema === schema && browse.data.table === table ? browse.data : undefined;

  const apiDraft = useMemo(() => toApiFilter(draft), [draft]);
  const debouncedDraft = useDebounced(apiDraft, 300);
  const preview = usePreview(
    filterOpen ? connection.id : null,
    filterOpen ? { schema, table, filter: debouncedDraft, sort, search: search || undefined, page_size: pageSize } : null,
  );
  const filterDirty = JSON.stringify(apiDraft) !== JSON.stringify(filter);

  const columns = useMemo(() => data?.columns ?? [], [data]);
  const gridColumns: GridColumn[] = useMemo(
    () => columns.map((column, index) => ({ column, index })).filter((c) => !hidden.includes(c.column.name)),
    [columns, hidden],
  );

  const setHiddenColumns = (next: string[]) => {
    setHidden(next);
    saveHidden(hiddenKey, next);
  };

  const toggleSort = (column: string) => {
    setSort((current) => {
      const existing = current.find((s) => s.column === column);
      if (!existing) return [{ column, direction: "asc" }];
      if (existing.direction === "asc") return [{ column, direction: "desc" }];
      return [];
    });
  };

  const applyFilter = () => {
    setFilter(apiDraft);
  };

  const selectedRows = useMemo(() => {
    if (!data) return [];
    return data.rows.filter((row) => {
      const ref = rowRef(data.columns, data.key_columns, row);
      return ref !== null && selected.has(ref.id);
    });
  }, [data, selected]);

  const copyRows = (format: "tsv" | "json") => {
    if (!data) return;
    const source = selectedRows.length > 0 ? selectedRows : data.rows;
    const names = gridColumns.map((c) => c.column.name);
    const rows = source.map((r) => gridColumns.map((c) => r[c.index]));
    const text = format === "tsv" ? rowsToTsv(names, rows) : rowsToJson(names, rows);
    void navigator.clipboard.writeText(text).then(
      () => toast.success(`Copied ${formatNumber(rows.length)} row${rows.length === 1 ? "" : "s"}`),
      () => toast.error("Clipboard access was denied by the browser."),
    );
  };

  const runExport = (format: ExportFormat, onlySelected: boolean) => {
    if (!data) return;
    const keys: ValueMap[] | undefined = onlySelected
      ? selectedRows.map((row) => rowRef(data.columns, data.key_columns, row)!.key)
      : undefined;
    const columnsToExport = hidden.length > 0 ? gridColumns.map((c) => c.column.name) : undefined;
    toast.promise(
      downloadExport(connection.id, {
        schema,
        table,
        format,
        filter,
        sort,
        search: search || undefined,
        keys,
        columns: columnsToExport,
      }),
      {
        loading: "Exporting…",
        success: "Export downloaded",
        error: (e) => (e instanceof ApiError ? e.message : "Export failed"),
      },
    );
  };

  const editable = data?.editable ?? false;
  const pending = changeCount(changes);
  const activeConditions = countFilterConditions(filter);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b bg-panel px-3 py-2">
        <TablePicker connectionId={connection.id} value={{ schema, table }} onChange={onPickTable} />
        {data && (
          <>
            {data.kind !== "table" && <Badge variant="outline">{data.kind.replace("_", " ")}</Badge>}
            {data.key_columns.length > 0 ? (
              <span className="hidden text-[11px] text-muted-foreground lg:inline">
                {data.key_kind === "primary_key" ? "PK" : "unique key"}: {data.key_columns.join(", ")}
              </span>
            ) : (
              <span className="text-[11px] text-muted-foreground">no row key</span>
            )}
          </>
        )}
        <div className="relative ml-auto w-48">
          <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <input
            value={searchInput}
            onChange={(e) => setSearchInput(e.target.value)}
            placeholder="Search rows…"
            aria-label="Search rows"
            className="h-7 w-full rounded-md border bg-background pr-6 pl-7 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring"
          />
          {searchInput && (
            <button
              type="button"
              onClick={() => setSearchInput("")}
              aria-label="Clear search"
              className="absolute top-1/2 right-1.5 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            >
              <X className="size-3" />
            </button>
          )}
        </div>
        <Button
          variant={filterOpen ? "secondary" : "ghost"}
          size="xs"
          onClick={() => setFilterOpen((o) => !o)}
          aria-expanded={filterOpen}
        >
          <Filter /> Filter
          {activeConditions > 0 && <span className="rounded bg-brand/15 px-1 text-brand tabular-nums">{activeConditions}</span>}
        </Button>
        <ColumnVisibility columns={columns} hidden={hidden} onChange={setHiddenColumns} />
        <DropdownMenu>
          <DropdownMenuTrigger render={<Button variant="ghost" size="xs" disabled={!data} />}>
            <Download /> Export
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-56">
            <DropdownMenuLabel>All matching rows</DropdownMenuLabel>
            {EXPORT_FORMATS.map(({ format, label }) => (
              <DropdownMenuItem key={format} onClick={() => runExport(format, false)}>
                {label}
              </DropdownMenuItem>
            ))}
            {selectedRows.length > 0 && (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuLabel>{selectedRows.length} selected rows</DropdownMenuLabel>
                {EXPORT_FORMATS.map(({ format, label }) => (
                  <DropdownMenuItem key={format} onClick={() => runExport(format, true)}>
                    {label}
                  </DropdownMenuItem>
                ))}
              </>
            )}
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={() => copyRows("tsv")}>
              <Copy /> Copy {selectedRows.length > 0 ? "selected" : "page"} as TSV
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => copyRows("json")}>
              <Copy /> Copy {selectedRows.length > 0 ? "selected" : "page"} as JSON
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
        {editable && data?.can_insert && (
          <Button variant="ghost" size="xs" onClick={() => onChanges((s) => addInsert(s))}>
            <Plus /> Row
          </Button>
        )}
        <Button
          variant="ghost"
          size="icon-xs"
          aria-label="Refresh"
          onClick={() => void browse.refetch()}
          disabled={browse.isFetching}
        >
          <RefreshCw className={browse.isFetching ? "animate-spin" : undefined} />
        </Button>
      </div>

      {data && !editable && data.read_only_reason && (
        <p className="flex items-center gap-2 border-b bg-muted/40 px-3 py-1.5 text-[11px] text-muted-foreground">
          <Lock className="size-3" /> {data.read_only_reason}
        </p>
      )}

      {filterOpen && columns.length > 0 && (
        <FilterBuilder
          columns={columns}
          value={draft}
          onChange={setDraft}
          onApply={applyFilter}
          onClear={() => {
            setDraft(newGroup("and", [newCondition()]));
            setFilter(null);
                  }}
          sql={preview.data?.select_sql}
          sqlError={preview.error?.message}
          dirty={filterDirty}
        />
      )}

      {browse.error && !data ? (
        <div role="alert" className="m-4 rounded-md border border-destructive/40 bg-destructive/10 p-3 text-xs">
          {browse.error.message}
        </div>
      ) : browse.error ? (
        <p role="alert" className="border-b bg-destructive/10 px-3 py-1.5 text-xs text-destructive">
          {browse.error.message}
        </p>
      ) : null}

      {data ? (
        <DataGrid
          columns={gridColumns}
          allColumns={data.columns}
          rows={data.rows}
          keyColumns={data.key_columns}
          rowOffset={page * pageSize}
          canUpdate={editable && data.can_update}
          canDelete={editable && data.can_delete}
          changes={changes}
          onChanges={onChanges}
          sort={sort}
          onSort={toggleSort}
          selected={selected}
          onSelectedChange={setSelected}
          failure={failure}
        />
      ) : (
        !browse.error && <div className="flex-1 p-6 text-center text-xs text-muted-foreground">Loading rows…</div>
      )}

      <div className="flex h-9 shrink-0 items-center gap-2 border-t bg-panel px-3 text-xs">
        <code className="hidden min-w-0 flex-1 truncate text-[11px] text-muted-foreground md:block" title={data?.sql}>
          {data?.sql}
        </code>
        {data && (
          <span className="shrink-0 text-muted-foreground tabular-nums">
            {data.total_rows !== null &&
              `${data.total_exact ? "" : "≈ "}${formatNumber(data.total_rows)} row${data.total_rows === 1 ? "" : "s"} · `}
            {data.execution_time_ms} ms{data.truncated && " · page cut at size limit"}
          </span>
        )}
        <div className="ml-auto flex shrink-0 items-center gap-1">
          <Select
            items={PAGE_SIZES.map((n) => ({ value: n, label: `${n} / page` }))}
            value={pageSize}
            onValueChange={(v) => {
              if (typeof v === "number") {
                setPageSize(v);
                          }
            }}
          >
            <SelectTrigger size="sm" className="h-6 text-xs" aria-label="Rows per page">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {PAGE_SIZES.map((n) => (
                <SelectItem key={n} value={n} className="text-xs">
                  {n} / page
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label="Previous page"
            disabled={page === 0}
            onClick={() => {
              setCursors(cursors.slice(0, -1));
            }}
          >
            <ChevronLeft />
          </Button>
          <span className="px-1 text-muted-foreground tabular-nums">Page {page + 1}</span>
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label="Next page"
            disabled={!data?.next_cursor || browse.isFetching}
            onClick={() => {
              const next = data?.next_cursor;
              if (next) {
                setCursors([...cursors, next]);
              }
            }}
          >
            <ChevronRight />
          </Button>
        </div>
      </div>

      {pending > 0 && (
        <ChangePanel
          connection={connection}
          schema={schema}
          table={table}
          changes={changes}
          onDiscard={() => {
            setFailure({});
            clearChanges(tableKey);
          }}
          onApplied={() => {
            setFailure({});
            clearChanges(tableKey);
          }}
          onFailure={setFailure}
        />
      )}
    </div>
  );
}
