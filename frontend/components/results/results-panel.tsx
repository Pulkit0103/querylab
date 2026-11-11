"use client";

import type { ReactNode } from "react";
import { AlertTriangle, GitBranch, Loader2, Table2, XCircle } from "lucide-react";

import { EmptyState } from "@/components/common/empty-state";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { formatDuration, formatNumber } from "@/lib/format";
import { useEditorStore, type ResultsView, type TabRuntime } from "@/stores/editor-store";

import { MessagesView } from "./messages-view";
import { PlanView } from "./plan-view";
import { ResultsGrid } from "./results-grid";

function ResultsBody({ runtime }: { runtime: TabRuntime }) {
  const { exec } = runtime;
  if (exec.status === "running" && exec.kind === "execute") {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-xs text-muted-foreground">
        <Loader2 className="size-4 animate-spin text-brand" /> Executing query…
      </div>
    );
  }
  if (exec.status === "error") {
    return (
      <EmptyState
        icon={XCircle}
        title="Query failed"
        description={<span className="font-mono text-destructive">{exec.error.message}</span>}
      />
    );
  }
  if (exec.status === "success") {
    const { result } = exec;
    if (result.columns.length === 0) {
      return (
        <EmptyState
          icon={Table2}
          title={result.command ? `${result.command} completed` : "Statement completed"}
          description={
            result.rows_affected !== null
              ? `${formatNumber(result.rows_affected)} rows affected in ${formatDuration(result.execution_time_ms)}.`
              : `Completed in ${formatDuration(result.execution_time_ms)}.`
          }
        />
      );
    }
    return (
      <div className="flex h-full min-h-0 flex-col">
        {result.truncated && (
          <div className="flex shrink-0 items-center gap-2 border-b bg-warning/10 px-3 py-1 text-xs text-warning">
            <AlertTriangle className="size-3.5" />
            Showing the first {formatNumber(result.row_count)} rows. Increase the row limit in Settings or add a
            LIMIT clause.
          </div>
        )}
        <div className="min-h-0 flex-1">
          <ResultsGrid key={exec.finishedAt} columns={result.columns} rows={result.rows} />
        </div>
      </div>
    );
  }
  return (
    <EmptyState
      icon={Table2}
      title="No results yet"
      description="Write a query and press Ctrl + Enter to run it."
    />
  );
}

function TabLabel({ children, badge }: { children: ReactNode; badge?: ReactNode }) {
  return (
    <span className="flex items-center gap-1.5">
      {children}
      {badge}
    </span>
  );
}

export function ResultsPanel({ tabId }: { tabId: string }) {
  const runtime = useEditorStore((s) => s.runtime[tabId]) ?? null;
  const setView = useEditorStore((s) => s.setView);
  const rt: TabRuntime = runtime ?? {
    exec: { status: "idle" },
    plan: null,
    planError: null,
    messages: [],
    view: "results",
  };
  const errorCount = rt.messages.filter((m) => m.level === "error").length;
  const success = rt.exec.status === "success" ? rt.exec.result : null;

  return (
    <Tabs
      value={rt.view}
      onValueChange={(v) => setView(tabId, v as ResultsView)}
      className="flex h-full min-h-0 flex-col gap-0 bg-background"
    >
      <div className="flex h-9 shrink-0 items-center border-b bg-panel pr-3">
        <TabsList variant="line" className="h-9 gap-0 rounded-none bg-transparent p-0 px-1">
          <TabsTrigger value="results" className="h-9 flex-none px-3 text-xs group-data-horizontal/tabs:after:bottom-0 after:bg-brand">
            <TabLabel>
              <Table2 className="size-3.5" /> Results
            </TabLabel>
          </TabsTrigger>
          <TabsTrigger value="messages" className="h-9 flex-none px-3 text-xs group-data-horizontal/tabs:after:bottom-0 after:bg-brand">
            <TabLabel
              badge={
                errorCount > 0 ? (
                  <span className="rounded-full bg-destructive/15 px-1.5 text-[10px] text-destructive">
                    {errorCount}
                  </span>
                ) : null
              }
            >
              Messages
            </TabLabel>
          </TabsTrigger>
          <TabsTrigger value="plan" className="h-9 flex-none px-3 text-xs group-data-horizontal/tabs:after:bottom-0 after:bg-brand">
            <TabLabel>
              <GitBranch className="size-3.5" /> Execution Plan
            </TabLabel>
          </TabsTrigger>
        </TabsList>
        {success && (
          <div className="ml-auto flex items-center gap-3 text-xs text-muted-foreground">
            <span>
              <span className="text-foreground tabular-nums">{formatNumber(success.row_count)}</span> rows
            </span>
            <span>
              <span className="text-foreground tabular-nums">{formatDuration(success.execution_time_ms)}</span>
            </span>
          </div>
        )}
      </div>
      <TabsContent value="results" className="min-h-0 flex-1">
        <ResultsBody runtime={rt} />
      </TabsContent>
      <TabsContent value="messages" className="min-h-0 flex-1">
        <MessagesView messages={rt.messages} />
      </TabsContent>
      <TabsContent value="plan" className="min-h-0 flex-1">
        <PlanView runtime={rt} />
      </TabsContent>
    </Tabs>
  );
}
