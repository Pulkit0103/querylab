"use client";

import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

import { NoConnection } from "@/components/explorer/no-connection";
import { SchemaTree } from "@/components/explorer/schema-tree";
import { ResultsPanel } from "@/components/results/results-panel";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { useActiveConnection } from "@/hooks/use-connections";
import { useMediaQuery } from "@/hooks/use-media-query";
import { useQueryRunner } from "@/hooks/use-query-runner";
import { useUpdateSavedQuery } from "@/hooks/use-library";
import { useCatalog } from "@/hooks/use-schema";
import { formatSql } from "@/lib/format";
import { previewSql } from "@/lib/sql-ident";
import { statementAt, trimStatement } from "@/lib/sql-statements";
import { useEditorStore } from "@/stores/editor-store";

import { EditorTabs } from "./editor-tabs";
import { EditorToolbar } from "./editor-toolbar";
import { ExplainMenu } from "./explain-analyze";
import { SaveQueryDialog } from "./save-query-dialog";
import { setCompletionCatalog } from "./sql-completion";
import { SqlEditor, type SqlEditorHandle } from "./sql-editor";

export function EditorWorkspace() {
  const tabs = useEditorStore((s) => s.tabs);
  const activeTabId = useEditorStore((s) => s.activeTabId);
  const updateSql = useEditorStore((s) => s.updateSql);
  const newTab = useEditorStore((s) => s.newTab);
  const exec = useEditorStore((s) => s.runtime[activeTabId]?.exec) ?? { status: "idle" as const };
  const activeTab = tabs.find((t) => t.id === activeTabId) ?? tabs[0];
  const [handle, setHandle] = useState<SqlEditorHandle | null>(null);
  const { run, cancel, explain: runExplain } = useQueryRunner();
  const isDesktop = useMediaQuery("(min-width: 768px)");
  const { connection, isPending: connectionsPending } = useActiveConnection();

  const runtime = useEditorStore((s) => s.runtime[activeTabId]);

  const catalog = useCatalog(connection?.id ?? null);
  useEffect(() => {
    if (handle) setCompletionCatalog(handle.monaco, catalog.data ?? []);
  }, [handle, catalog.data]);

  /**
   * The text to execute: the selection if any, otherwise the statement under
   * the cursor. The executed range is briefly highlighted.
   */
  const currentTarget = useCallback((): { sql: string; offset: number } => {
    const editor = handle?.editor;
    const model = editor?.getModel();
    if (!editor || !model) return { sql: activeTab.sql, offset: 0 };
    const selection = editor.getSelection();
    let target: { sql: string; offset: number } | null = null;
    if (selection && !selection.isEmpty()) {
      const text = model.getValueInRange(selection);
      if (text.trim()) target = { sql: text, offset: model.getOffsetAt(selection.getStartPosition()) };
    }
    if (!target) {
      const position = editor.getPosition();
      const statement = statementAt(model.getValue(), position ? model.getOffsetAt(position) : 0);
      if (!statement) return { sql: "", offset: 0 };
      const { text, offset } = trimStatement(statement);
      target = { sql: text, offset };
    }
    const start = model.getPositionAt(target.offset);
    const end = model.getPositionAt(target.offset + target.sql.length);
    const decorations = editor.createDecorationsCollection([
      {
        range: { startLineNumber: start.lineNumber, startColumn: start.column, endLineNumber: end.lineNumber, endColumn: end.column },
        options: { className: "ql-executed-range", isWholeLine: false },
      },
    ]);
    setTimeout(() => decorations.clear(), 700);
    return target;
  }, [handle, activeTab.sql]);

  const runCurrent = useCallback(() => {
    const { sql, offset } = currentTarget();
    void run(activeTab.id, sql, { offset });
  }, [currentTarget, run, activeTab.id]);

  // Show the server-reported error position as a marker in the editor.
  useEffect(() => {
    const editor = handle?.editor;
    const monaco = handle?.monaco;
    const model = editor?.getModel();
    if (!monaco || !model) return;
    const error =
      runtime?.exec.status === "error" ? runtime.exec.error : runtime?.exec.status === "idle" ? runtime.planError : null;
    const details = (error?.details ?? {}) as { position?: number | null; sqlOffset?: number };
    if (!error || typeof details.position !== "number") {
      monaco.editor.setModelMarkers(model, "querylab", []);
      return;
    }
    const start = model.getPositionAt((details.sqlOffset ?? 0) + details.position - 1);
    const word = model.getWordAtPosition(start);
    monaco.editor.setModelMarkers(model, "querylab", [
      {
        severity: monaco.MarkerSeverity.Error,
        message: error.message,
        startLineNumber: start.lineNumber,
        startColumn: word?.startColumn ?? start.column,
        endLineNumber: start.lineNumber,
        endColumn: word?.endColumn ?? start.column + 1,
      },
    ]);
  }, [handle, runtime?.exec, runtime?.planError]);

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

  const [saveOpen, setSaveOpen] = useState(false);
  const updateSaved = useUpdateSavedQuery();
  const markSaved = useEditorStore((s) => s.markSaved);

  const save = useCallback(() => {
    if (!activeTab.sql.trim()) {
      toast.error("Nothing to save — the editor is empty.");
      return;
    }
    if (activeTab.savedQueryId) {
      const { id: tabId, title, sql, savedQueryId } = activeTab;
      updateSaved.mutate(
        { id: savedQueryId, body: { sql_content: sql } },
        {
          onSuccess: (saved) => {
            markSaved(tabId, saved.id, title);
            toast.success(`Saved “${saved.name}”`);
          },
          onError: (e) => toast.error(e.status === 404 ? "This saved query no longer exists." : e.message),
        },
      );
      return;
    }
    setSaveOpen(true);
  }, [activeTab, updateSaved, markSaved]);

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

  const explainTarget = useCallback(
    ({ sql, offset }: { sql: string; offset: number }, analyze: boolean) => {
      void runExplain(activeTab.id, sql, { offset, analyze });
    },
    [runExplain, activeTab.id],
  );
  const explain = useCallback(() => explainTarget(currentTarget(), false), [currentTarget, explainTarget]);

  const editorArea = (
    <ResizablePanelGroup orientation="vertical">
      <ResizablePanel defaultSize="58%" minSize="20%">
        <div className="flex h-full min-h-0 flex-col">
          <EditorTabs />
          <EditorToolbar
            exec={exec}
            canRun={activeTab.sql.trim().length > 0}
            onRun={runCurrent}
            onCancel={() => cancel(activeTab.id)}
            onExplain={explain}
            onFormat={format}
            onSave={save}
            onCopy={copy}
            onClear={clear}
            explainExtra={
              <ExplainMenu
                disabled={exec.status === "running" || activeTab.sql.trim().length === 0}
                getTarget={currentTarget}
                onExplain={explainTarget}
              />
            }
          />
          <div className="min-h-0 flex-1">
            <SqlEditor
              path={`querylab://tab/${activeTab.id}.sql`}
              value={activeTab.sql}
              onChange={(v) => updateSql(activeTab.id, v)}
              onRun={runCurrent}
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
      <SaveQueryDialog
        open={saveOpen}
        onOpenChange={setSaveOpen}
        defaultName={activeTab.title}
        sql={activeTab.sql}
        connectionId={connection?.id ?? null}
        onSaved={(saved) => markSaved(activeTab.id, saved.id, saved.name)}
      />
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
