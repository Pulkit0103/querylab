"use client";

import { useCallback, useMemo, useRef, useState, type KeyboardEvent } from "react";
import {
  ArrowDown,
  ArrowUp,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Copy,
  Download,
  Search,
  X,
} from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { compareCells, NUMERIC_TYPES } from "@/lib/compare-cells";
import { downloadText, toCsv, toTsv } from "@/lib/csv";
import { cellToText, formatNumber } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { CellValue, ColumnMeta } from "@/types/query";

const PAGE_SIZES = [50, 100, 250, 500];

type SortState = { col: number; dir: "asc" | "desc" } | null;
interface CellPos {
  r: number;
  c: number;
}

function Cell({ value, numeric }: { value: CellValue; numeric: boolean }) {
  if (value === null) return <span className="text-muted-foreground/50 italic">NULL</span>;
  if (typeof value === "boolean")
    return <span className={value ? "text-success" : "text-destructive/80"}>{String(value)}</span>;
  if (typeof value === "object") return <span className="text-chart-2">{JSON.stringify(value)}</span>;
  return <span className={numeric ? "text-chart-3" : undefined}>{String(value)}</span>;
}

export function ResultsGrid({ columns, rows }: { columns: ColumnMeta[]; rows: CellValue[][] }) {
  const [sort, setSort] = useState<SortState>(null);
  const [filter, setFilter] = useState("");
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(100);
  const [anchor, setAnchor] = useState<CellPos | null>(null);
  const [focus, setFocus] = useState<CellPos | null>(null);
  const dragging = useRef(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // View state (sort, page, selection) is reset by the parent remounting this
  // component with a new `key` whenever a new result set arrives.
  const numericCols = useMemo(() => columns.map((c) => NUMERIC_TYPES.has(c.type)), [columns]);

  const processed = useMemo(() => {
    let out = rows;
    const needle = filter.trim().toLowerCase();
    if (needle) {
      out = out.filter((row) => row.some((v) => cellToText(v).toLowerCase().includes(needle)));
    }
    if (sort) {
      const { col, dir } = sort;
      const numeric = numericCols[col];
      out = [...out].sort((a, b) => {
        const cmp = compareCells(a[col], b[col], numeric);
        return dir === "asc" ? cmp : -cmp;
      });
    }
    return out;
  }, [rows, filter, sort, numericCols]);

  const pageCount = Math.max(1, Math.ceil(processed.length / pageSize));
  const safePage = Math.min(page, pageCount - 1);
  const offset = safePage * pageSize;
  const pageRows = processed.slice(offset, offset + pageSize);

  const range = useMemo(() => {
    if (!anchor || !focus) return null;
    return {
      r0: Math.min(anchor.r, focus.r),
      r1: Math.max(anchor.r, focus.r),
      c0: Math.min(anchor.c, focus.c),
      c1: Math.max(anchor.c, focus.c),
    };
  }, [anchor, focus]);

  const copySelection = useCallback(async () => {
    const source = range
      ? pageRows.slice(range.r0, range.r1 + 1).map((row) => row.slice(range.c0, range.c1 + 1))
      : processed;
    try {
      await navigator.clipboard.writeText(toTsv(source));
      const cells = source.length * (source[0]?.length ?? 0);
      toast.success(`Copied ${formatNumber(cells)} cell${cells === 1 ? "" : "s"}`);
    } catch {
      toast.error("Clipboard access was denied by the browser.");
    }
  }, [range, pageRows, processed]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c" && range) {
      e.preventDefault();
      void copySelection();
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      setAnchor({ r: 0, c: 0 });
      setFocus({ r: pageRows.length - 1, c: columns.length - 1 });
    }
    if (e.key === "Escape") {
      setAnchor(null);
      setFocus(null);
    }
  };

  const toggleSort = (col: number) => {
    setSort((s) => {
      if (!s || s.col !== col) return { col, dir: "asc" };
      if (s.dir === "asc") return { col, dir: "desc" };
      return null;
    });
    setPage(0);
  };

  const exportCsv = () => {
    const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
    downloadText(`querylab-results-${stamp}.csv`, toCsv(columns, processed));
  };

  const isSelected = (r: number, c: number) =>
    range !== null && r >= range.r0 && r <= range.r1 && c >= range.c0 && c <= range.c1;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div
        ref={containerRef}
        tabIndex={0}
        onKeyDown={onKeyDown}
        onMouseUp={() => (dragging.current = false)}
        onMouseLeave={() => (dragging.current = false)}
        className="min-h-0 flex-1 overflow-auto outline-none scrollbar-thin"
        aria-label="Query results"
      >
        <table className="w-max min-w-full border-separate border-spacing-0 font-mono text-xs">
          <thead className="sticky top-0 z-20">
            <tr>
              <th className="sticky left-0 z-30 w-12 min-w-12 border-r border-b bg-panel px-2 text-right font-normal text-muted-foreground/60">
                #
              </th>
              {columns.map((col, i) => (
                <th
                  key={`${col.name}-${i}`}
                  scope="col"
                  className="border-r border-b bg-panel p-0 text-left font-normal"
                  aria-sort={
                    sort?.col === i ? (sort.dir === "asc" ? "ascending" : "descending") : "none"
                  }
                >
                  <button
                    type="button"
                    onClick={() => toggleSort(i)}
                    className="group flex w-full min-w-24 items-center gap-2 px-2.5 py-1.5 hover:bg-accent/60"
                  >
                    <span className="flex min-w-0 flex-col items-start">
                      <span className="truncate font-sans text-[12px] font-medium text-foreground">
                        {col.name || "?column?"}
                      </span>
                      <span className="text-[10px] text-muted-foreground/70">{col.type}</span>
                    </span>
                    <span className="ml-auto text-muted-foreground">
                      {sort?.col === i ? (
                        sort.dir === "asc" ? (
                          <ArrowUp className="size-3" />
                        ) : (
                          <ArrowDown className="size-3" />
                        )
                      ) : (
                        <ArrowUp className="size-3 opacity-0 group-hover:opacity-40" />
                      )}
                    </span>
                  </button>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {pageRows.map((row, r) => (
              <tr key={offset + r} className="group/row">
                <td
                  className="sticky left-0 z-10 cursor-pointer border-r border-b bg-panel px-2 text-right text-muted-foreground/60 tabular-nums select-none group-hover/row:text-foreground"
                  onClick={() => {
                    setAnchor({ r, c: 0 });
                    setFocus({ r, c: columns.length - 1 });
                    containerRef.current?.focus();
                  }}
                >
                  {offset + r + 1}
                </td>
                {row.map((value, c) => (
                  <td
                    key={c}
                    onMouseDown={(e) => {
                      if (e.button !== 0) return;
                      if (e.shiftKey && anchor) {
                        setFocus({ r, c });
                      } else {
                        setAnchor({ r, c });
                        setFocus({ r, c });
                      }
                      dragging.current = true;
                      containerRef.current?.focus();
                    }}
                    onMouseEnter={() => {
                      if (dragging.current) setFocus({ r, c });
                    }}
                    title={value !== null && typeof value !== "boolean" ? cellToText(value).slice(0, 2000) : undefined}
                    className={cn(
                      "max-w-96 truncate border-r border-b px-2.5 py-1 select-none",
                      numericCols[c] && "text-right tabular-nums",
                      isSelected(r, c) ? "bg-brand/15 outline outline-1 -outline-offset-1 outline-brand/40" : "group-hover/row:bg-accent/40",
                    )}
                  >
                    <Cell value={value} numeric={numericCols[c]} />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
        {pageRows.length === 0 && (
          <div className="p-6 text-center text-xs text-muted-foreground">
            {rows.length === 0 ? "The query returned no rows." : "No rows match the current filter."}
          </div>
        )}
      </div>

      <div className="flex h-9 shrink-0 items-center gap-2 border-t bg-panel px-2 text-xs">
        <div className="relative w-56 max-w-[40%]">
          <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <input
            value={filter}
            onChange={(e) => {
              setFilter(e.target.value);
              setPage(0);
            }}
            placeholder="Filter rows…"
            aria-label="Filter rows"
            className="h-6 w-full rounded-md border bg-background pr-6 pl-7 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring"
          />
          {filter && (
            <button
              type="button"
              onClick={() => setFilter("")}
              aria-label="Clear filter"
              className="absolute top-1/2 right-1.5 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            >
              <X className="size-3" />
            </button>
          )}
        </div>
        <span className="hidden text-muted-foreground sm:inline">
          {filter ? `${formatNumber(processed.length)} of ${formatNumber(rows.length)}` : formatNumber(rows.length)} rows
        </span>

        <div className="ml-auto flex items-center gap-1">
          <Button variant="ghost" size="xs" onClick={() => void copySelection()} className="gap-1">
            <Copy className="size-3" /> {range ? "Copy selection" : "Copy all"}
          </Button>
          <Button variant="ghost" size="xs" onClick={exportCsv} className="gap-1" disabled={processed.length === 0}>
            <Download className="size-3" /> CSV
          </Button>
          <Select
            items={PAGE_SIZES.map((n) => ({ value: n, label: `${n} / page` }))}
            value={pageSize}
            onValueChange={(v) => {
              if (typeof v === "number") {
                setPageSize(v);
                setPage(0);
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
          <Button variant="ghost" size="icon-xs" onClick={() => setPage(0)} disabled={safePage === 0} aria-label="First page">
            <ChevronsLeft />
          </Button>
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={() => setPage(safePage - 1)}
            disabled={safePage === 0}
            aria-label="Previous page"
          >
            <ChevronLeft />
          </Button>
          <span className="px-1 tabular-nums text-muted-foreground">
            {safePage + 1} / {pageCount}
          </span>
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={() => setPage(safePage + 1)}
            disabled={safePage >= pageCount - 1}
            aria-label="Next page"
          >
            <ChevronRight />
          </Button>
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={() => setPage(pageCount - 1)}
            disabled={safePage >= pageCount - 1}
            aria-label="Last page"
          >
            <ChevronsRight />
          </Button>
        </div>
      </div>
    </div>
  );
}
