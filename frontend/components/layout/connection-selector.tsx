"use client";

import { Database } from "lucide-react";

import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

/** Active database selector. Phase 2 exposes the demo dataset only. */
export function ConnectionSelector() {
  const items = [{ value: "demo", label: "Demo database" }];
  return (
    <Select items={items} value="demo">
      <SelectTrigger size="sm" className="h-7 min-w-40 text-xs" aria-label="Active database">
        <Database className="size-3.5 text-muted-foreground" />
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {items.map((i) => (
          <SelectItem key={i.value} value={i.value} className="text-xs">
            {i.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
