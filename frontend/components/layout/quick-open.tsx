"use client";

import { useDeferredValue, useEffect, useMemo, useState, type KeyboardEvent } from "react";
import { useRouter } from "next/navigation";
import { Bookmark, History, Pin, Search } from "lucide-react";

import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { useHistory, useSavedQueries } from "@/hooks/use-library";
import { cn } from "@/lib/utils";
import { useEditorStore } from "@/stores/editor-store";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { HistoryEntry, SavedQuery } from "@/types/library";

type Item =
  | { kind: "saved"; key: string; query: SavedQuery }
  | { kind: "history"; key: string; entry: HistoryEntry };

const MAX_ITEMS = 30;
const oneLine = (sql: string) => sql.replace(/\s+/g, " ").trim();

/** Builds the result list: pinned first, then matching saved queries. */
export function quickOpenItems(saved: SavedQuery[], pinnedHistory: HistoryEntry[], query: string): Item[] {
  const needle = query.trim().toLowerCase();
  const matches = (text: string) => !needle || text.toLowerCase().includes(needle);
  const pinnedSaved = saved.filter((q) => q.pinned);
  const otherSaved = needle ? saved.filter((q) => !q.pinned) : [];
  const items: Item[] = [
    ...pinnedSaved.filter((q) => matches(`${q.name} ${q.tags.join(" ")} ${q.sql_content}`)).map((query) => ({ kind: "saved" as const, key: `s-${query.id}`, query })),
    ...pinnedHistory.filter((e) => matches(e.sql_content)).map((entry) => ({ kind: "history" as const, key: `h-${entry.id}`, entry })),
    ...otherSaved
      .filter((q) => matches(`${q.name} ${q.tags.join(" ")} ${q.sql_content}`))
      .map((query) => ({ kind: "saved" as const, key: `s-${query.id}`, query })),
  ];
  return items.slice(0, MAX_ITEMS);
}

/**
 * Ctrl+P (⌘P): jump to a pinned or saved query, or a pinned history entry,
 * and open it in the editor.
 */
export function QuickOpen() {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const deferred = useDeferredValue(query.trim());
  const openSaved = useEditorStore((s) => s.openSavedQuery);
  const newTab = useEditorStore((s) => s.newTab);
  const setActiveConnection = useWorkspaceStore((s) => s.setActiveConnection);

  // Pinned saved queries come first in the server's order; a search adds the
  // rest of the matching saved queries.
  const saved = useSavedQueries(deferred ? { q: deferred } : { pinned: true }, open);
  const pinnedHistory = useHistory({ pinned: true }, open);
  const items = useMemo(
    () => quickOpenItems(saved.data ?? [], pinnedHistory.data?.pages.flatMap((p) => p.items) ?? [], deferred),
    [saved.data, pinnedHistory.data, deferred],
  );

  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "p") {
        e.preventDefault();
        e.stopPropagation();
        setQuery("");
        setCursor(0);
        setOpen(true);
      }
    };
    // Capture phase: runs before Monaco and the browser's print shortcut.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const choose = (item: Item) => {
    setOpen(false);
    if (item.kind === "saved") {
      if (item.query.connection_id) setActiveConnection(item.query.connection_id);
      openSaved({ id: item.query.id, name: item.query.name, sql: item.query.sql_content });
    } else {
      if (item.entry.connection_id) setActiveConnection(item.entry.connection_id);
      const first = oneLine(item.entry.sql_content);
      newTab({ title: first.length > 40 ? `${first.slice(0, 40)}…` : first || "History", sql: item.entry.sql_content });
    }
    router.push("/editor");
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursor((c) => Math.min(c + 1, items.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursor((c) => Math.max(c - 1, 0));
    } else if (e.key === "Enter" && items[cursor]) {
      e.preventDefault();
      choose(items[cursor]);
    }
  };

  const loading = saved.isPending || pinnedHistory.isPending;
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="gap-0 p-0 sm:max-w-xl" showCloseButton={false}>
        <DialogTitle className="sr-only">Open a query</DialogTitle>
        <DialogDescription className="sr-only">Search pinned and saved queries and pinned history.</DialogDescription>
        <div className="flex items-center gap-2 border-b px-3">
          <Search className="size-4 text-muted-foreground" />
          <input
            autoFocus
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setCursor(0);
            }}
            onKeyDown={onKeyDown}
            placeholder="Open a pinned or saved query…"
            aria-label="Search queries"
            className="h-11 flex-1 bg-transparent text-sm outline-none"
          />
          <kbd className="rounded border bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">Esc</kbd>
        </div>
        <ul role="listbox" aria-label="Queries" className="max-h-96 overflow-auto py-1 scrollbar-thin">
          {items.map((item, i) => {
            const pinned = item.kind === "history" || item.query.pinned;
            return (
              <li
                key={item.key}
                role="option"
                aria-selected={i === cursor}
                onMouseEnter={() => setCursor(i)}
                onClick={() => choose(item)}
                className={cn("flex cursor-pointer items-center gap-3 px-3 py-2 text-sm", i === cursor && "bg-accent")}
              >
                {item.kind === "saved" ? (
                  <Bookmark className="size-4 shrink-0 text-brand" />
                ) : (
                  <History className="size-4 shrink-0 text-muted-foreground" />
                )}
                <span className="min-w-0 flex-1">
                  <span className="block truncate">
                    {item.kind === "saved" ? item.query.name : oneLine(item.entry.sql_content)}
                  </span>
                  <span className="block truncate font-mono text-[11px] text-muted-foreground">
                    {item.kind === "saved"
                      ? item.query.tags.length > 0
                        ? item.query.tags.map((t) => `#${t}`).join(" ")
                        : oneLine(item.query.sql_content)
                      : (item.entry.connection_name ?? "deleted connection")}
                  </span>
                </span>
                {pinned && <Pin className="size-3.5 shrink-0 text-warning" aria-label="Pinned" />}
              </li>
            );
          })}
          {items.length === 0 && (
            <li className="px-3 py-6 text-center text-xs text-muted-foreground">
              {loading ? "Loading…" : deferred ? "No matching queries." : "Pin saved queries or history entries to see them here."}
            </li>
          )}
        </ul>
      </DialogContent>
    </Dialog>
  );
}
