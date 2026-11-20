"use client";

import { useRouter } from "next/navigation";
import { Bookmark, History, Pin } from "lucide-react";

import { PinButton } from "@/components/library/pin-button";
import { Skeleton } from "@/components/ui/skeleton";
import { useHistory, useSavedQueries } from "@/hooks/use-library";
import { formatRelativeTime } from "@/lib/format";
import { useEditorStore } from "@/stores/editor-store";
import { useWorkspaceStore } from "@/stores/workspace-store";

const oneLine = (sql: string) => sql.replace(/\s+/g, " ").trim();

/** Pinned saved queries and history entries, opened in the editor in one click. */
export function PinnedPanel() {
  const router = useRouter();
  const saved = useSavedQueries({ pinned: true });
  const history = useHistory({ pinned: true });
  const openSaved = useEditorStore((s) => s.openSavedQuery);
  const newTab = useEditorStore((s) => s.newTab);
  const setActiveConnection = useWorkspaceStore((s) => s.setActiveConnection);
  const entries = history.data?.pages.flatMap((p) => p.items) ?? [];

  if (saved.isPending || history.isPending) return <Skeleton className="h-24" />;
  if ((saved.data ?? []).length === 0 && entries.length === 0) {
    return (
      <p className="flex items-center gap-2 text-xs text-muted-foreground">
        <Pin className="size-3.5" /> Pin saved queries or history entries to keep them here. Press Ctrl+P anywhere to open
        them.
      </p>
    );
  }

  return (
    <ul className="divide-y text-xs">
      {(saved.data ?? []).map((q) => (
        <li key={q.id} className="flex items-center gap-3 py-1.5">
          <Bookmark className="size-3.5 shrink-0 text-brand" />
          <button
            type="button"
            className="min-w-0 flex-1 truncate text-left text-sm hover:text-brand"
            onClick={() => {
              if (q.connection_id) setActiveConnection(q.connection_id);
              openSaved({ id: q.id, name: q.name, sql: q.sql_content });
              router.push("/editor");
            }}
          >
            {q.name}
          </button>
          <span className="hidden truncate text-muted-foreground sm:inline">{q.tags.map((t) => `#${t}`).join(" ")}</span>
          <PinButton target="saved" id={q.id} pinned label={q.name} />
        </li>
      ))}
      {entries.map((h) => (
        <li key={h.id} className="flex items-center gap-3 py-1.5">
          <History className="size-3.5 shrink-0 text-muted-foreground" />
          <button
            type="button"
            className="min-w-0 flex-1 truncate text-left font-mono hover:text-brand"
            onClick={() => {
              if (h.connection_id) setActiveConnection(h.connection_id);
              const first = oneLine(h.sql_content);
              newTab({ title: first.length > 40 ? `${first.slice(0, 40)}…` : first, sql: h.sql_content });
              router.push("/editor");
            }}
          >
            {oneLine(h.sql_content)}
          </button>
          <span className="shrink-0 text-muted-foreground">{formatRelativeTime(h.executed_at)}</span>
          <PinButton target="history" id={h.id} pinned label="history entry" />
        </li>
      ))}
    </ul>
  );
}
