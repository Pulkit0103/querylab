"use client";

import { Keyboard } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";

const SHORTCUTS: [string, string][] = [
  ["Ctrl Enter", "Run selection or statement under cursor"],
  ["Ctrl S", "Save query"],
  ["Ctrl Shift F", "Format SQL"],
  ["Ctrl /", "Toggle line comment"],
  ["Ctrl F", "Find"],
  ["Ctrl H", "Find and replace"],
  ["Ctrl Space", "Trigger autocomplete"],
  ["Ctrl Z / Ctrl Y", "Undo / redo"],
  ["Alt ↑ / Alt ↓", "Move line up / down"],
  ["Ctrl D", "Select next occurrence"],
  ["Ctrl C", "Copy selected result cells"],
];

export function ShortcutsPopover() {
  return (
    <Popover>
      <PopoverTrigger render={<Button variant="ghost" size="icon-sm" aria-label="Keyboard shortcuts" />}>
        <Keyboard className="size-3.5" />
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80 p-0">
        <p className="border-b px-3 py-2 text-xs font-medium">Keyboard shortcuts</p>
        <ul className="divide-y text-xs">
          {SHORTCUTS.map(([keys, label]) => (
            <li key={keys} className="flex items-center justify-between gap-3 px-3 py-1.5">
              <span className="text-muted-foreground">{label}</span>
              <kbd className="shrink-0 rounded border bg-muted px-1.5 py-0.5 font-mono text-[10px]">{keys}</kbd>
            </li>
          ))}
        </ul>
        <p className="border-t px-3 py-2 text-[11px] text-muted-foreground">Use ⌘ instead of Ctrl on macOS.</p>
      </PopoverContent>
    </Popover>
  );
}
