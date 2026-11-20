"use client";

import { Columns3 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { DataColumn } from "@/types/data";

/** Show or hide grid columns; the choice is saved per table in the browser. */
export function ColumnVisibility({
  columns,
  hidden,
  onChange,
}: {
  columns: DataColumn[];
  hidden: string[];
  onChange: (hidden: string[]) => void;
}) {
  const shown = columns.length - columns.filter((c) => hidden.includes(c.name)).length;
  return (
    <Popover>
      <PopoverTrigger render={<Button variant="ghost" size="xs" aria-label="Choose columns" />}>
        <Columns3 /> Columns
        {hidden.length > 0 && (
          <span className="text-muted-foreground tabular-nums">
            {shown}/{columns.length}
          </span>
        )}
      </PopoverTrigger>
      <PopoverContent align="end" className="w-64 p-0">
        <div className="flex items-center justify-between border-b px-3 py-2 text-xs">
          <span className="font-medium">Visible columns</span>
          <Button variant="ghost" size="xs" onClick={() => onChange([])} disabled={hidden.length === 0}>
            Show all
          </Button>
        </div>
        <ul className="max-h-72 overflow-auto py-1 text-xs scrollbar-thin">
          {columns.map((c) => {
            const visible = !hidden.includes(c.name);
            return (
              <li key={c.name}>
                <label className="flex cursor-pointer items-center gap-2 px-3 py-1 hover:bg-accent/60">
                  <Checkbox
                    checked={visible}
                    // Keep at least one column visible.
                    disabled={visible && shown === 1}
                    onCheckedChange={(v) =>
                      onChange(v === true ? hidden.filter((h) => h !== c.name) : [...hidden, c.name])
                    }
                  />
                  <span className="truncate">{c.name}</span>
                  <span className="ml-auto text-[10px] text-muted-foreground">{c.data_type}</span>
                </label>
              </li>
            );
          })}
        </ul>
      </PopoverContent>
    </Popover>
  );
}
