"use client";

import { useMemo, useState } from "react";
import { ListChecks } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { LARGE_SCHEMA_TABLES } from "@/lib/diagram";

/** Server limit on tables per diagram. */
export const MAX_SELECTED_TABLES = 500;

/**
 * Chooses which tables the diagram shows. `null` means the whole schema
 * (the most connected tables for large schemas).
 */
export function TableSelect({
  available,
  shown,
  selection,
  onChange,
}: {
  /** Every table in the schema. */
  available: string[];
  /** Tables currently in the diagram. */
  shown: string[];
  selection: string[] | null;
  onChange: (tables: string[] | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState("");

  const visible = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return q ? available.filter((t) => t.toLowerCase().includes(q)) : available;
  }, [available, filter]);

  const onOpenChange = (next: boolean) => {
    if (next) {
      setDraft(new Set(selection ?? shown));
      setFilter("");
    }
    setOpen(next);
  };

  const toggle = (name: string, on: boolean) =>
    setDraft((d) => {
      const next = new Set(d);
      if (on) next.add(name);
      else next.delete(name);
      return next;
    });

  const tooMany = draft.size > MAX_SELECTED_TABLES;
  const apply = () => {
    // "Everything" is the default view unless the schema is large, where the
    // default is only the most connected tables.
    const all = available.length > 0 && available.every((t) => draft.has(t));
    onChange(all && available.length <= LARGE_SCHEMA_TABLES ? null : available.filter((t) => draft.has(t)));
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger render={<Button variant="outline" size="sm" className="h-7 gap-1.5 text-xs" />}>
        <ListChecks className="size-3.5" />
        Tables
        <span className="text-muted-foreground tabular-nums">
          {shown.length}/{available.length || shown.length}
        </span>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-72 p-0">
        <div className="space-y-2 border-b p-2">
          <Input
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filter tables…"
            aria-label="Filter tables"
            className="h-7 text-xs"
          />
          <div className="flex items-center gap-1 text-xs">
            <Button
              variant="ghost"
              size="xs"
              onClick={() => setDraft((d) => new Set([...d, ...visible]))}
              disabled={visible.length === 0}
            >
              Select {filter ? "matching" : "all"}
            </Button>
            <Button
              variant="ghost"
              size="xs"
              onClick={() => setDraft((d) => new Set([...d].filter((t) => !visible.includes(t))))}
              disabled={visible.length === 0}
            >
              Clear {filter ? "matching" : "all"}
            </Button>
            <span className="ml-auto text-muted-foreground tabular-nums">{draft.size} selected</span>
          </div>
        </div>
        <ul className="max-h-72 overflow-auto py-1 text-xs scrollbar-thin" aria-label="Tables">
          {visible.map((name) => (
            <li key={name}>
              <label className="flex cursor-pointer items-center gap-2 px-3 py-1 hover:bg-accent/60">
                <Checkbox checked={draft.has(name)} onCheckedChange={(v) => toggle(name, v === true)} />
                <span className="truncate font-mono">{name}</span>
              </label>
            </li>
          ))}
          {visible.length === 0 && <li className="px-3 py-2 text-muted-foreground">No tables match.</li>}
        </ul>
        <div className="flex items-center gap-2 border-t p-2">
          {tooMany && (
            <p role="alert" className="text-xs text-destructive">
              At most {MAX_SELECTED_TABLES} tables.
            </p>
          )}
          <Button variant="ghost" size="xs" className="ml-auto" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button size="xs" onClick={apply} disabled={draft.size === 0 || tooMany}>
            Show {draft.size} table{draft.size === 1 ? "" : "s"}
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
