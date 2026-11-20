"use client";

import { FolderPlus, Plus, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import {
  arityOf,
  isGroup,
  MAX_FILTER_DEPTH,
  newCondition,
  newGroup,
  OPERATORS,
  operatorsFor,
  updateNode,
  type UiCondition,
  type UiGroup,
} from "@/lib/data-studio";
import { cn } from "@/lib/utils";
import type { DataColumn, FilterOperator } from "@/types/data";

interface FilterBuilderProps {
  columns: DataColumn[];
  value: UiGroup;
  onChange: (value: UiGroup) => void;
  onApply: () => void;
  onClear: () => void;
  /** Server-rendered SELECT for the draft filter. */
  sql?: string;
  sqlError?: string;
  /** The draft differs from the filter that is applied. */
  dirty: boolean;
}

const inputClass =
  "h-7 min-w-0 rounded-md border bg-background px-2 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring";

function ConditionRow({
  condition,
  columns,
  onChange,
  onRemove,
  onEnter,
}: {
  condition: UiCondition;
  columns: DataColumn[];
  onChange: (c: UiCondition) => void;
  onRemove: () => void;
  onEnter: () => void;
}) {
  const column = columns.find((c) => c.name === condition.column);
  const operators = operatorsFor(column?.kind);
  const arity = arityOf(condition.operator);
  const valueInput = (field: "value" | "value2", placeholder: string) =>
    column?.kind === "boolean" ? (
      <Select
        items={[
          { value: "true", label: "true" },
          { value: "false", label: "false" },
        ]}
        value={condition[field] || null}
        onValueChange={(v) => onChange({ ...condition, [field]: String(v ?? "") })}
      >
        <SelectTrigger size="sm" className="h-7 w-24 text-xs" aria-label="Value">
          <SelectValue placeholder="value" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="true">true</SelectItem>
          <SelectItem value="false">false</SelectItem>
        </SelectContent>
      </Select>
    ) : (
      <input
        aria-label={field === "value" ? "Value" : "Upper bound"}
        value={condition[field]}
        placeholder={placeholder}
        list={column?.kind === "enum" ? `enum-${condition.id}` : undefined}
        onChange={(e) => onChange({ ...condition, [field]: e.target.value })}
        onKeyDown={(e) => e.key === "Enter" && onEnter()}
        className={cn(inputClass, "w-36 font-mono")}
      />
    );

  return (
    <div className="flex flex-wrap items-center gap-1.5" data-testid="filter-condition">
      <Select
        items={columns.map((c) => ({ value: c.name, label: c.name }))}
        value={condition.column || null}
        onValueChange={(v) => {
          const name = String(v ?? "");
          const kind = columns.find((c) => c.name === name)?.kind;
          const ops = operatorsFor(kind);
          onChange({ ...condition, column: name, operator: ops.includes(condition.operator) ? condition.operator : ops[0] });
        }}
      >
        <SelectTrigger size="sm" className="h-7 w-40 text-xs" aria-label="Column">
          <SelectValue placeholder="column" />
        </SelectTrigger>
        <SelectContent>
          {columns.map((c) => (
            <SelectItem key={c.name} value={c.name} className="text-xs">
              {c.name} <span className="ml-auto text-[10px] text-muted-foreground">{c.data_type}</span>
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select
        items={OPERATORS.filter((o) => operators.includes(o.value))}
        value={condition.operator}
        onValueChange={(v) => v && onChange({ ...condition, operator: v as FilterOperator })}
      >
        <SelectTrigger size="sm" className="h-7 w-28 text-xs" aria-label="Operator">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {OPERATORS.filter((o) => operators.includes(o.value)).map((o) => (
            <SelectItem key={o.value} value={o.value} className="text-xs">
              {o.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {arity === 1 && valueInput("value", condition.operator.includes("like") ? "%pattern%" : "value")}
      {arity === "list" && valueInput("value", "a, b, c")}
      {arity === 2 && (
        <>
          {valueInput("value", "from")}
          <span className="text-xs text-muted-foreground">and</span>
          {valueInput("value2", "to")}
        </>
      )}
      {column?.kind === "enum" && (
        <datalist id={`enum-${condition.id}`}>
          {column.enum_values.map((v) => (
            <option key={v} value={v} />
          ))}
        </datalist>
      )}
      <Button variant="ghost" size="icon-xs" onClick={onRemove} aria-label="Remove condition">
        <X />
      </Button>
    </div>
  );
}

function GroupEditor({
  group,
  depth,
  columns,
  root,
  onChange,
  onEnter,
}: {
  group: UiGroup;
  depth: number;
  columns: DataColumn[];
  root: UiGroup;
  onChange: (root: UiGroup) => void;
  onEnter: () => void;
}) {
  const edit = (fn: (g: UiGroup) => UiGroup) =>
    onChange(group.id === root.id ? fn(root) : updateNode(root, group.id, (n) => fn(n as UiGroup)));

  return (
    <div
      className={cn("space-y-1.5", depth > 0 && "rounded-md border border-dashed p-2")}
      role="group"
      aria-label={depth === 0 ? "Filter conditions" : "Condition group"}
    >
      {group.items.length > 1 && (
        <div className="flex items-center gap-1 text-[11px] text-muted-foreground">
          Match
          {(["and", "or"] as const).map((op) => (
            <button
              key={op}
              type="button"
              aria-pressed={group.op === op}
              onClick={() => edit((g) => ({ ...g, op }))}
              className={cn(
                "rounded px-1.5 py-0.5 font-medium uppercase",
                group.op === op ? "bg-brand/15 text-brand" : "hover:bg-accent",
              )}
            >
              {op === "and" ? "all (AND)" : "any (OR)"}
            </button>
          ))}
        </div>
      )}
      {group.items.map((item) =>
        isGroup(item) ? (
          <div key={item.id} className="flex items-start gap-1">
            <div className="min-w-0 flex-1">
              <GroupEditor group={item} depth={depth + 1} columns={columns} root={root} onChange={onChange} onEnter={onEnter} />
            </div>
            <Button
              variant="ghost"
              size="icon-xs"
              onClick={() => onChange(updateNode(root, item.id, () => null))}
              aria-label="Remove group"
            >
              <X />
            </Button>
          </div>
        ) : (
          <ConditionRow
            key={item.id}
            condition={item}
            columns={columns}
            onEnter={onEnter}
            onChange={(c) => onChange(updateNode(root, item.id, () => c))}
            onRemove={() => onChange(updateNode(root, item.id, () => null))}
          />
        ),
      )}
      <div className="flex gap-1">
        <Button variant="ghost" size="xs" onClick={() => edit((g) => ({ ...g, items: [...g.items, newCondition()] }))}>
          <Plus /> Condition
        </Button>
        {depth + 1 < MAX_FILTER_DEPTH && (
          <Button
            variant="ghost"
            size="xs"
            onClick={() =>
              edit((g) => ({ ...g, items: [...g.items, newGroup(g.op === "and" ? "or" : "and", [newCondition()])] }))
            }
          >
            <FolderPlus /> Group
          </Button>
        )}
      </div>
    </div>
  );
}

/** Visual filter builder with a live, server-rendered SQL preview. */
export function FilterBuilder({ columns, value, onChange, onApply, onClear, sql, sqlError, dirty }: FilterBuilderProps) {
  return (
    <div className="space-y-2 border-b bg-panel/60 px-3 py-2">
      <GroupEditor group={value} depth={0} columns={columns} root={value} onChange={onChange} onEnter={onApply} />
      <div className="flex flex-wrap items-center gap-2">
        <pre
          aria-label="Generated SQL"
          className={cn(
            "min-w-0 flex-1 truncate rounded-md border bg-background px-2 py-1 font-mono text-[11px]",
            sqlError ? "text-destructive" : "text-muted-foreground",
          )}
          title={sqlError ?? sql}
        >
          {sqlError ?? sql ?? "…"}
        </pre>
        <Button variant="ghost" size="xs" onClick={onClear}>
          Clear
        </Button>
        <Button size="xs" onClick={onApply} disabled={!dirty}>
          Apply filter
        </Button>
      </div>
    </div>
  );
}
