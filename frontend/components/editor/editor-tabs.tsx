"use client";

import { useState, type KeyboardEvent } from "react";
import { FileCode2, Loader2, Plus, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { isTabDirty, useEditorStore } from "@/stores/editor-store";

export function EditorTabs() {
  const tabs = useEditorStore((s) => s.tabs);
  const activeTabId = useEditorStore((s) => s.activeTabId);
  const runtime = useEditorStore((s) => s.runtime);
  const setActive = useEditorStore((s) => s.setActiveTab);
  const closeTab = useEditorStore((s) => s.closeTab);
  const newTab = useEditorStore((s) => s.newTab);
  const renameTab = useEditorStore((s) => s.renameTab);
  const [editing, setEditing] = useState<string | null>(null);

  const commitRename = (id: string, value: string) => {
    const title = value.trim();
    if (title) renameTab(id, title.slice(0, 120));
    setEditing(null);
  };

  return (
    <div className="flex h-9 shrink-0 items-stretch border-b bg-panel">
      <div className="flex min-w-0 flex-1 items-stretch overflow-x-auto scrollbar-thin" role="tablist">
        {tabs.map((tab) => {
          const active = tab.id === activeTabId;
          const running = runtime[tab.id]?.exec.status === "running";
          const dirty = isTabDirty(tab);
          return (
            <div
              key={tab.id}
              role="tab"
              aria-selected={active}
              tabIndex={0}
              onClick={() => setActive(tab.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter") setActive(tab.id);
              }}
              onDoubleClick={() => setEditing(tab.id)}
              onAuxClick={(e) => {
                if (e.button === 1) closeTab(tab.id);
              }}
              className={cn(
                "group relative flex max-w-52 min-w-28 cursor-pointer items-center gap-1.5 border-r px-3 text-xs text-muted-foreground transition-colors select-none",
                "hover:bg-accent/50 hover:text-foreground",
                active && "bg-background text-foreground",
              )}
            >
              {active && <span className="absolute inset-x-0 top-0 h-px bg-brand" />}
              {running ? (
                <Loader2 className="size-3.5 shrink-0 animate-spin text-brand" />
              ) : (
                <FileCode2 className={cn("size-3.5 shrink-0", active && "text-brand")} />
              )}
              {editing === tab.id ? (
                <input
                  autoFocus
                  defaultValue={tab.title}
                  aria-label="Tab name"
                  className="h-5 w-full min-w-0 rounded-sm bg-muted px-1 text-xs text-foreground outline-none ring-1 ring-ring"
                  onClick={(e) => e.stopPropagation()}
                  onBlur={(e) => commitRename(tab.id, e.currentTarget.value)}
                  onKeyDown={(e: KeyboardEvent<HTMLInputElement>) => {
                    if (e.key === "Enter") commitRename(tab.id, e.currentTarget.value);
                    if (e.key === "Escape") setEditing(null);
                  }}
                />
              ) : (
                <span className="truncate">{tab.title}</span>
              )}
              <span className="ml-auto flex size-4 shrink-0 items-center justify-center">
                {dirty && <span className="size-1.5 rounded-full bg-foreground/60 group-hover:hidden" />}
                <button
                  type="button"
                  aria-label={`Close ${tab.title}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    closeTab(tab.id);
                  }}
                  className={cn(
                    "rounded-sm p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground",
                    dirty ? "hidden group-hover:block" : "opacity-0 group-hover:opacity-100",
                    active && !dirty && "opacity-100",
                  )}
                >
                  <X className="size-3" />
                </button>
              </span>
            </div>
          );
        })}
      </div>
      <Button
        variant="ghost"
        size="icon-sm"
        className="m-1 shrink-0"
        onClick={() => newTab()}
        aria-label="New query tab"
      >
        <Plus className="size-4" />
      </Button>
    </div>
  );
}
