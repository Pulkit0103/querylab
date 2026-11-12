"use client";

import { useCallback, useState } from "react";
import { toast } from "sonner";

import { NoConnection } from "@/components/explorer/no-connection";
import { SchemaTree } from "@/components/explorer/schema-tree";
import { ResultsPanel } from "@/components/results/results-panel";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { useActiveConnection } from "@/hooks/use-connections";
import { useMediaQuery } from "@/hooks/use-media-query";
import { useQueryRunner } from "@/hooks/use-query-runner";
import { formatSql } from "@/lib/format";
import { previewSql } from "@/lib/sql-ident";
import { useEditorStore } from "@/stores/editor-store";

import { EditorTabs } from "./editor-tabs";
import { EditorToolbar } from "./editor-toolbar";
import { SqlEditor, type SqlEditorHandle } from "./sql-editor";

export function EditorWorkspace() {
  const tabs = useEditorStore((s) => s.tabs);
  const activeTabId = useEditorStore((s) => s.activeTabId);
  const updateSql = useEditorStore((s) => s.updateSql);
  const newTab = useEditorStore((s) => s.newTab);
  const exec = useEditorStore((s) => s.runtime[activeTabId]?.exec) ?? { status: "idle" as const };
  const activeTab = tabs.find((t) => t.id === activeTabId) ?? tabs[0];
  const [handle, setHandle] = useState<SqlEditorHandle | null>(null);
  const { run, cancel } = useQueryRunner();
  const isDesktop = useMediaQuery("(min-width: 768px)");
  const { connection, isPending: connectionsPending } = useActiveConnection();

  const runSql = useCallback((sql: string) => void run(activeTab.id, sql), [run, activeTab.id]);

  const runFromToolbar = useCallback(() => {
    const editor = handle?.editor;
    const model = editor?.getModel();
    const selection = editor?.getSelection();
    const selected = model && selection && !selection.isEmpty() ? model.getValueInRange(selection) : "";
    runSql(selected.trim() ? selected : activeTab.sql);
  }, [handle, runSql, activeTab.sql]);

  const format = useCallback(() => {
    try {
      const formatted = formatSql(activeTab.sql);
      const editor = handle?.editor;
      const model = editor?.getModel();
      if (editor && model) {
        // Use an edit operation so formatting is undoable with Ctrl+Z.
        editor.pushUndoStop();
        editor.executeEdits("format", [{ range: model.getFullModelRange(), text: formatted }]);
        editor.pushUndoStop();
      } else {
        updateSql(activeTab.id, formatted);
      }
    } catch {
      toast.error("Could not format SQL — check the statement for syntax errors.");
    }
  }, [activeTab, handle, updateSql]);

  const save = useCallback(() => {
    toast.info("Saved queries become available once the API is connected.");
  }, []);

  const copy = useCallback(() => {
    void navigator.clipboard.writeText(activeTab.sql).then(
      () => toast.success("Query copied to clipboard"),
      () => toast.error("Clipboard access was denied by the browser."),
    );
  }, [activeTab.sql]);

  const clear = useCallback(() => {
    const editor = handle?.editor;
    const model = editor?.getModel();
    if (editor && model) {
      editor.pushUndoStop();
      editor.executeEdits("clear", [{ range: model.getFullModelRange(), text: "" }]);
      editor.pushUndoStop();
      editor.focus();
    } else {
      updateSql(activeTab.id, "");
    }
  }, [activeTab.id, handle, updateSql]);

  const explain = useCallback(() => {
    toast.info("Execution plans become available once the API is connected.");
  }, []);

  const editorArea = (
    <ResizablePanelGroup orientation="vertical">
      <ResizablePanel defaultSize="58%" minSize="20%">
        <div className="flex h-full min-h-0 flex-col">
          <EditorTabs />
          <EditorToolbar
            exec={exec}
            canRun={activeTab.sql.trim().length > 0}
            onRun={runFromToolbar}
            onCancel={() => cancel(activeTab.id)}
            onExplain={explain}
            onFormat={format}
            onSave={save}
            onCopy={copy}
            onClear={clear}
          />
          <div className="min-h-0 flex-1">
            <SqlEditor
              path={`querylab://tab/${activeTab.id}.sql`}
              value={activeTab.sql}
              onChange={(v) => updateSql(activeTab.id, v)}
              onRun={runSql}
              onSave={save}
              onFormat={format}
              onMount={setHandle}
            />
          </div>
        </div>
      </ResizablePanel>
      <ResizableHandle />
      <ResizablePanel defaultSize="42%" minSize="12%" collapsible collapsedSize="0%">
        <ResultsPanel tabId={activeTab.id} />
      </ResizablePanel>
    </ResizablePanelGroup>
  );

  if (!isDesktop) return <div className="h-full">{editorArea}</div>;

  return (
    <ResizablePanelGroup orientation="horizontal" className="h-full">
      <ResizablePanel defaultSize="20%" minSize="180px" maxSize="40%" collapsible collapsedSize="0%" className="bg-panel">
        {connection ? (
          <SchemaTree
            connectionId={connection.id}
            databaseName={connection.database}
            onPreviewTable={(schema, table) => {
              const id = newTab({ title: table, sql: previewSql(schema, table) });
              void run(id, previewSql(schema, table));
            }}
          />
        ) : (
          !connectionsPending && <NoConnection compact />
        )}
      </ResizablePanel>
      <ResizableHandle />
      <ResizablePanel defaultSize="80%" minSize="40%">
        {editorArea}
      </ResizablePanel>
    </ResizablePanelGroup>
  );
}
