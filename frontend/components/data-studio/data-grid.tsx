"use client";

import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { ArrowDown, ArrowUp, KeyRound, Trash2, Undo2, X } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  removeInsert,
  rowRef,
  setInsertValue,
  stageEdit,
  toggleDelete,
  type RowRef,
  type StagedChanges,
} from "@/lib/data-studio";
import { cn } from "@/lib/utils";
import type { DataCell, DataColumn, SortKey } from "@/types/data";

export interface GridColumn {
  column: DataColumn;
  /** Position of the column in each row array. */
  index: number;
}

interface DataGridProps {
  columns: GridColumn[];
  allColumns: DataColumn[];
  rows: DataCell[][];
  keyColumns: string[];
  /** Row number of the first row (for display). */
  rowOffset: number;
  canUpdate: boolean;
  canDelete: boolean;
  changes: StagedChanges;
  onChanges: (fn: (s: StagedChanges) => StagedChanges) => void;
  sort: SortKey[];
  onSort: (column: string) => void;
  selected: ReadonlySet<string>;
  onSelectedChange: (ids: Set<string>) => void;
  /** Row of a change that failed to apply. */
  failure?: { rowId?: string; insertId?: string };
}

type Target = { kind: "row"; ref: RowRef; row: DataCell[] } | { kind: "insert"; id: string };
type Editing = { target: Target; column: DataColumn; index: number } | null;

const has = (obj: object, key: string) => Object.prototype.hasOwnProperty.call(obj, key);
const targetId = (t: Target) => (t.kind === "row" ? t.ref.id : t.id);

const editorClass =
  "w-full rounded border bg-background px-1.5 py-1 font-mono text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring";

function CellEditor({
  column,
  initial,
  onCommit,
  onCancel,
  onDefault,
}: {
  column: DataColumn;
  initial: DataCell;
  onCommit: (value: DataCell) => void;
  onCancel: () => void;
  onDefault?: () => void;
}) {
  const [value, setValue] = useState(initial ?? "");
  const multiline = column.kind === "json" || (initial?.length ?? 0) > 60 || (initial ?? "").includes("\n");
  const ref = useRef<HTMLInputElement & HTMLTextAreaElement & HTMLSelectElement>(null);
  useEffect(() => ref.current?.focus(), []);

  const onKeyDown = (e: KeyboardEvent) => {
    e.stopPropagation();
    if (e.key === "Escape") onCancel();
    if (e.key === "Enter" && (!multiline || e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      onCommit(value);
    }
  };

  const choices = column.kind === "boolean" ? ["true", "false"] : column.kind === "enum" ? column.enum_values : null;
  return (
    <div
      className="absolute top-0 left-0 z-40 w-72 space-y-1.5 rounded-md border bg-popover p-2 shadow-lg"
      role="dialog"
      aria-label={`Edit ${column.name}`}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <div className="flex items-center justify-between text-[10px] text-muted-foreground">
        <span className="truncate font-medium">{column.name}</span>
        <span>{column.data_type}</span>
      </div>
      {choices ? (
        <select ref={ref} value={value} onChange={(e) => setValue(e.target.value)} onKeyDown={onKeyDown} className={editorClass} aria-label="Value">
          {!choices.includes(value) && <option value={value}>{value || "(choose)"}</option>}
          {choices.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
      ) : multiline ? (
        <textarea
          ref={ref}
          rows={6}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={onKeyDown}
          className={editorClass}
          aria-label="Value"
        />
      ) : (
        <input ref={ref} value={value} onChange={(e) => setValue(e.target.value)} onKeyDown={onKeyDown} className={editorClass} aria-label="Value" />
      )}
      <div className="flex items-center gap-1">
        {column.nullable && (
          <Button variant="ghost" size="xs" onClick={() => onCommit(null)}>
            NULL
          </Button>
        )}
        {onDefault && (
          <Button variant="ghost" size="xs" onClick={onDefault}>
            Default
          </Button>
        )}
        <span className="ml-auto text-[10px] text-muted-foreground">{multiline ? "Ctrl+Enter" : "Enter"}</span>
        <Button variant="ghost" size="xs" onClick={onCancel}>
          Cancel
        </Button>
        <Button size="xs" onClick={() => onCommit(value)}>
          Save
        </Button>
      </div>
    </div>
  );
}

function CellText({ value, column, placeholder }: { value: DataCell | undefined; column: DataColumn; placeholder?: string }) {
  if (value === undefined) return <span className="text-muted-foreground/60 italic">{placeholder}</span>;
  if (value === null) return <span className="text-muted-foreground/50 italic">NULL</span>;
  if (column.kind === "boolean")
    return <span className={value === "true" ? "text-success" : "text-destructive/80"}>{value}</span>;
  return <span className={cn(column.kind === "number" && "text-chart-3", column.kind === "json" && "text-chart-2")}>{value}</span>;
}

/** Editable, server-paginated grid for one table. */
export function DataGrid({
  columns,
  allColumns,
  rows,
  keyColumns,
  rowOffset,
  canUpdate,
  canDelete,
  changes,
  onChanges,
  sort,
  onSort,
  selected,
  onSelectedChange,
  failure,
}: DataGridProps) {
  const [editing, setEditing] = useState<Editing>(null);
  const [active, setActive] = useState<{ target: Target; index: number } | null>(null);

  const refs = rows.map((row) => rowRef(allColumns, keyColumns, row));
  const selectable = refs.filter((r): r is RowRef => r !== null);
  const allSelected = selectable.length > 0 && selectable.every((r) => selected.has(r.id));

  const valueOf = (target: Target, index: number): DataCell | undefined => {
    const name = allColumns[index].name;
    if (target.kind === "insert") {
      const values = changes.inserts.find((r) => r.id === target.id)?.values ?? {};
      return has(values, name) ? values[name] : undefined;
    }
    const edit = changes.edits[target.ref.id];
    return edit && has(edit.values, name) ? edit.values[name] : target.row[index];
  };

  const canEdit = (target: Target, column: DataColumn) =>
    column.editable && (target.kind === "insert" || (canUpdate && !changes.deletes[target.ref.id]));

  const startEdit = (target: Target, gc: GridColumn) => {
    if (canEdit(target, gc.column)) setEditing({ target, column: gc.column, index: gc.index });
  };

  const commit = (value: DataCell | undefined) => {
    if (!editing) return;
    const { target, column, index } = editing;
    if (target.kind === "insert") onChanges((s) => setInsertValue(s, target.id, column.name, value));
    else if (value !== undefined) onChanges((s) => stageEdit(s, target.ref, column.name, value, target.row[index]));
    setEditing(null);
  };

  const copyActive = () => {
    if (!active) return;
    const value = valueOf(active.target, active.index);
    void navigator.clipboard.writeText(value ?? "").then(
      () => toast.success("Copied cell"),
      () => toast.error("Clipboard access was denied by the browser."),
    );
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (editing) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c" && active) {
      e.preventDefault();
      copyActive();
    }
    if ((e.key === "Enter" || e.key === "F2") && active) {
      const gc = columns.find((c) => c.index === active.index);
      if (gc) {
        e.preventDefault();
        startEdit(active.target, gc);
      }
    }
  };

  const sortOf = (name: string) => sort.find((s) => s.column === name);

  const renderCells = (target: Target, deleted: boolean) =>
    columns.map((gc) => {
      const value = valueOf(target, gc.index);
      const staged =
        target.kind === "row"
          ? has(changes.edits[target.ref.id]?.values ?? {}, gc.column.name)
          : value !== undefined;
      const isActive = active && targetId(active.target) === targetId(target) && active.index === gc.index;
      const isEditing = editing && targetId(editing.target) === targetId(target) && editing.index === gc.index;
      const placeholder =
        gc.column.generated || gc.column.identity || gc.column.has_default ? "DEFAULT" : gc.column.nullable ? "NULL" : "required";
      return (
        <td
          key={gc.index}
          onMouseDown={() => setActive({ target, index: gc.index })}
          onDoubleClick={() => startEdit(target, gc)}
          data-staged={staged || undefined}
          title={value ?? undefined}
          className={cn(
            "relative max-w-96 truncate border-r border-b px-2.5 py-1",
            gc.column.kind === "number" && "text-right tabular-nums",
            staged && "bg-warning/15",
            deleted && "line-through opacity-60",
            canEdit(target, gc.column) && "cursor-text",
            isActive && "outline outline-1 -outline-offset-1 outline-brand",
          )}
        >
          <CellText value={value} column={gc.column} placeholder={placeholder} />
          {isEditing && editing && (
            <CellEditor
              column={gc.column}
              initial={value ?? null}
              onCommit={commit}
              onCancel={() => setEditing(null)}
              onDefault={target.kind === "insert" ? () => commit(undefined) : undefined}
            />
          )}
        </td>
      );
    });

  return (
    <div
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="min-h-0 flex-1 overflow-auto outline-none scrollbar-thin"
      aria-label="Table data"
    >
      <table className="w-max min-w-full border-separate border-spacing-0 font-mono text-xs">
        <thead className="sticky top-0 z-20">
          <tr>
            <th className="sticky left-0 z-30 w-20 min-w-20 border-r border-b bg-panel px-2 text-left font-normal">
              <Checkbox
                aria-label="Select all rows on this page"
                checked={allSelected}
                disabled={selectable.length === 0}
                onCheckedChange={(v) => onSelectedChange(new Set(v === true ? selectable.map((r) => r.id) : []))}
              />
            </th>
            {columns.map(({ column }) => {
              const s = sortOf(column.name);
              return (
                <th
                  key={column.name}
                  scope="col"
                  className="border-r border-b bg-panel p-0 text-left font-normal"
                  aria-sort={s ? (s.direction === "asc" ? "ascending" : "descending") : "none"}
                >
                  <button
                    type="button"
                    onClick={() => onSort(column.name)}
                    className="group flex w-full min-w-28 items-center gap-2 px-2.5 py-1.5 hover:bg-accent/60"
                  >
                    <span className="flex min-w-0 flex-col items-start">
                      <span className="flex items-center gap-1 truncate font-sans text-[12px] font-medium text-foreground">
                        {column.is_key && <KeyRound className="size-3 text-warning" aria-label="Key column" />}
                        {column.name}
                      </span>
                      <span className="text-[10px] text-muted-foreground/70">
                        {column.data_type}
                        {!column.editable && " · read-only"}
                      </span>
                    </span>
                    <span className="ml-auto text-muted-foreground">
                      {s ? (
                        s.direction === "asc" ? <ArrowUp className="size-3" /> : <ArrowDown className="size-3" />
                      ) : (
                        <ArrowUp className="size-3 opacity-0 group-hover:opacity-40" />
                      )}
                    </span>
                  </button>
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {changes.inserts.map((insert) => {
            const target: Target = { kind: "insert", id: insert.id };
            return (
              <tr
                key={insert.id}
                data-row-state="inserted"
                className={cn("bg-success/10", failure?.insertId === insert.id && "outline outline-2 -outline-offset-2 outline-destructive")}
              >
                <td className="sticky left-0 z-10 border-r border-b bg-success/20 px-2 text-[10px] text-success">
                  <span className="flex items-center gap-1">
                    NEW
                    <Button
                      variant="ghost"
                      size="icon-xs"
                      className="ml-auto"
                      aria-label="Remove new row"
                      onClick={() => onChanges((s) => removeInsert(s, insert.id))}
                    >
                      <X />
                    </Button>
                  </span>
                </td>
                {renderCells(target, false)}
              </tr>
            );
          })}
          {rows.map((row, r) => {
            const ref = refs[r];
            const target: Target | null = ref ? { kind: "row", ref, row } : null;
            const deleted = ref ? Boolean(changes.deletes[ref.id]) : false;
            return (
              <tr
                key={ref?.id ?? `row-${rowOffset + r}`}
                data-row-state={deleted ? "deleted" : undefined}
                className={cn(
                  "group/row",
                  deleted && "bg-destructive/10",
                  ref && failure?.rowId === ref.id && "outline outline-2 -outline-offset-2 outline-destructive",
                )}
              >
                <td className="sticky left-0 z-10 border-r border-b bg-panel px-2 text-muted-foreground/60 tabular-nums">
                  <span className="flex items-center gap-1.5">
                    {ref && (
                      <Checkbox
                        aria-label={`Select row ${rowOffset + r + 1}`}
                        checked={selected.has(ref.id)}
                        onCheckedChange={(v) => {
                          const next = new Set(selected);
                          if (v === true) next.add(ref.id);
                          else next.delete(ref.id);
                          onSelectedChange(next);
                        }}
                      />
                    )}
                    <span className="text-[10px]">{rowOffset + r + 1}</span>
                    {ref && canDelete && (
                      <Button
                        variant="ghost"
                        size="icon-xs"
                        className="ml-auto opacity-0 group-hover/row:opacity-100 focus-visible:opacity-100"
                        aria-label={deleted ? `Keep row ${rowOffset + r + 1}` : `Delete row ${rowOffset + r + 1}`}
                        onClick={() => onChanges((s) => toggleDelete(s, ref))}
                      >
                        {deleted ? <Undo2 /> : <Trash2 />}
                      </Button>
                    )}
                  </span>
                </td>
                {target ? (
                  renderCells(target, deleted)
                ) : (
                  columns.map((gc) => (
                    <td key={gc.index} className="max-w-96 truncate border-r border-b px-2.5 py-1" title={row[gc.index] ?? undefined}>
                      <CellText value={row[gc.index]} column={gc.column} />
                    </td>
                  ))
                )}
              </tr>
            );
          })}
        </tbody>
      </table>
      {rows.length === 0 && changes.inserts.length === 0 && (
        <div className="p-6 text-center text-xs text-muted-foreground">No rows match.</div>
      )}
    </div>
  );
}
