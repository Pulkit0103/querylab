"use client";

import { useDeferredValue, useState } from "react";
import { useRouter } from "next/navigation";
import {
  CheckCircle2,
  Clock,
  History,
  Loader2,
  Play,
  Search,
  SquareArrowOutUpRight,
  Timer,
  Trash2,
  XCircle,
  Ban,
} from "lucide-react";
import { toast } from "sonner";

import { EmptyState } from "@/components/common/empty-state";
import { PageContainer } from "@/components/common/page-container";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useClearHistory, useDeleteHistory, useHistory } from "@/hooks/use-library";
import { useQueryRunner } from "@/hooks/use-query-runner";
import { formatDuration, formatNumber, formatRelativeTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useEditorStore } from "@/stores/editor-store";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { HistoryEntry, HistoryStatus } from "@/types/library";

const STATUS_META: Record<HistoryStatus, { label: string; icon: typeof CheckCircle2; className: string }> = {
  success: { label: "Success", icon: CheckCircle2, className: "text-success" },
  error: { label: "Error", icon: XCircle, className: "text-destructive" },
  timeout: { label: "Timed out", icon: Timer, className: "text-warning" },
  cancelled: { label: "Cancelled", icon: Ban, className: "text-muted-foreground" },
};

const FILTERS = [
  { value: "all", label: "All statuses" },
  { value: "success", label: "Success" },
  { value: "error", label: "Error" },
  { value: "timeout", label: "Timed out" },
  { value: "cancelled", label: "Cancelled" },
];

function titleFor(sql: string): string {
  const first = sql.trim().split("\n")[0] ?? "";
  return first.length > 40 ? `${first.slice(0, 40)}…` : first || "History";
}

function HistoryRow({ entry }: { entry: HistoryEntry }) {
  const router = useRouter();
  const newTab = useEditorStore((s) => s.newTab);
  const setActiveConnection = useWorkspaceStore((s) => s.setActiveConnection);
  const del = useDeleteHistory();
  const { run } = useQueryRunner();
  const meta = STATUS_META[entry.status];

  const open = (andRun: boolean) => {
    if (entry.connection_id) setActiveConnection(entry.connection_id);
    const id = newTab({ title: titleFor(entry.sql_content), sql: entry.sql_content });
    if (andRun) {
      if (!entry.connection_id) {
        toast.error("The connection used for this query was deleted.");
      } else {
        void run(id, entry.sql_content);
      }
    }
    router.push("/editor");
  };

  return (
    <li className="group flex gap-3 px-4 py-3 hover:bg-accent/30">
      <Tooltip>
        <TooltipTrigger render={<span className="mt-0.5" />}>
          <meta.icon className={cn("size-4", meta.className)} aria-label={meta.label} />
        </TooltipTrigger>
        <TooltipContent>{meta.label}</TooltipContent>
      </Tooltip>
      <div className="min-w-0 flex-1">
        <pre className="line-clamp-3 font-mono text-xs break-all whitespace-pre-wrap text-foreground">{entry.sql_content}</pre>
        {entry.error_message && <p className="mt-1 line-clamp-2 text-xs text-destructive">{entry.error_message}</p>}
        <p className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
          <span className="flex items-center gap-1">
            <Clock className="size-3" />
            <time dateTime={entry.executed_at} title={new Date(entry.executed_at).toLocaleString()}>
              {formatRelativeTime(entry.executed_at)}
            </time>
          </span>
          <span className="tabular-nums">{formatDuration(entry.execution_time_ms)}</span>
          {entry.row_count !== null && (
            <span className="tabular-nums">
              {formatNumber(entry.row_count)} {entry.row_count === 1 ? "row" : "rows"}
            </span>
          )}
          <span>{entry.connection_name ?? <em>deleted connection</em>}</span>
        </p>
      </div>
      <div className="flex shrink-0 items-start gap-1 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
        <Button size="sm" variant="ghost" className="gap-1.5" onClick={() => open(true)} disabled={!entry.connection_id}>
          <Play className="size-3" /> Run again
        </Button>
        <Button size="icon-sm" variant="ghost" aria-label="Open in editor" onClick={() => open(false)}>
          <SquareArrowOutUpRight className="size-3.5" />
        </Button>
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label="Delete history entry"
          className="hover:text-destructive"
          onClick={() => del.mutate(entry.id, { onError: (e) => toast.error(e.message) })}
        >
          <Trash2 className="size-3.5" />
        </Button>
      </div>
    </li>
  );
}

export default function HistoryPage() {
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("all");
  const q = useDeferredValue(search.trim());
  const history = useHistory({
    ...(q ? { q } : {}),
    ...(status !== "all" ? { status: status as HistoryStatus } : {}),
  });
  const clear = useClearHistory();
  const items = history.data?.pages.flatMap((p) => p.items) ?? [];

  return (
    <PageContainer
      title="Query History"
      description="Every statement you have executed, newest first."
      actions={
        <Button
          size="sm"
          variant="outline"
          className="gap-1.5"
          disabled={items.length === 0 || clear.isPending}
          onClick={() => {
            if (window.confirm("Delete your entire query history? This cannot be undone.")) {
              clear.mutate(undefined, { onSuccess: (r) => toast.success(`Deleted ${formatNumber(r.deleted)} entries`) });
            }
          }}
        >
          <Trash2 className="size-3.5" /> Clear history
        </Button>
      }
    >
      <div className="mb-3 flex flex-wrap gap-2">
        <div className="relative min-w-56 flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Search SQL…" className="pl-8" aria-label="Search history" />
        </div>
        <Select items={FILTERS} value={status} onValueChange={(v) => setStatus(String(v ?? "all"))}>
          <SelectTrigger className="w-40" aria-label="Filter by status">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {FILTERS.map((f) => (
              <SelectItem key={f.value} value={f.value}>
                {f.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      <div className="rounded-lg border bg-card">
        {history.isPending ? (
          <div className="space-y-3 p-4">
            {Array.from({ length: 4 }).map((_, i) => (
              <Skeleton key={i} className="h-14" />
            ))}
          </div>
        ) : history.isError ? (
          <p className="p-4 text-sm text-destructive">{history.error.message}</p>
        ) : items.length === 0 ? (
          <EmptyState
            className="py-16"
            icon={History}
            title={q || status !== "all" ? "No matching queries" : "No history yet"}
            description={q || status !== "all" ? "Try a different search or filter." : "Queries you run will be recorded here."}
          />
        ) : (
          <ul className="divide-y">
            {items.map((e) => (
              <HistoryRow key={e.id} entry={e} />
            ))}
          </ul>
        )}
      </div>
      {history.hasNextPage && (
        <div className="mt-3 flex justify-center">
          <Button variant="outline" size="sm" onClick={() => void history.fetchNextPage()} disabled={history.isFetchingNextPage}>
            {history.isFetchingNextPage && <Loader2 className="size-3.5 animate-spin" />} Load more
          </Button>
        </div>
      )}
    </PageContainer>
  );
}
