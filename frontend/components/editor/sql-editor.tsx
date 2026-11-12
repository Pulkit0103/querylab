"use client";

import { useEffect, useRef } from "react";
import Editor, { type BeforeMount, type OnMount } from "@monaco-editor/react";
import { useTheme } from "next-themes";

import { Skeleton } from "@/components/ui/skeleton";

export type MonacoEditor = Parameters<OnMount>[0];
export type MonacoApi = Parameters<OnMount>[1];

export interface SqlEditorHandle {
  editor: MonacoEditor;
  monaco: MonacoApi;
}

interface SqlEditorProps {
  /** Stable model path; one Monaco model (and undo stack) per tab. */
  path: string;
  value: string;
  onChange: (value: string) => void;
  /** Ctrl+Enter / context menu "Run Query"; the caller decides what to run. */
  onRun: () => void;
  onSave: () => void;
  onFormat: () => void;
  onMount?: (handle: SqlEditorHandle) => void;
}

const defineThemes: BeforeMount = (monaco) => {
  monaco.editor.defineTheme("querylab-dark", {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "keyword", foreground: "6ED3BF", fontStyle: "bold" },
      { token: "operator", foreground: "9AA4B2" },
      { token: "string", foreground: "E6C07B" },
      { token: "number", foreground: "D19A66" },
      { token: "comment", foreground: "5C6370", fontStyle: "italic" },
      { token: "predefined", foreground: "82AAFF" },
      { token: "identifier", foreground: "D6DAE0" },
    ],
    colors: {
      "editor.background": "#17181B",
      "editor.foreground": "#D6DAE0",
      "editorLineNumber.foreground": "#4A4F57",
      "editorLineNumber.activeForeground": "#9AA4B2",
      "editor.lineHighlightBackground": "#1E2024",
      "editor.lineHighlightBorder": "#00000000",
      "editor.selectionBackground": "#2E4A45",
      "editor.inactiveSelectionBackground": "#26302E",
      "editorCursor.foreground": "#6ED3BF",
      "editorIndentGuide.background1": "#24262A",
      "editorBracketMatch.background": "#2E4A4566",
      "editorBracketMatch.border": "#6ED3BF66",
      "editorWidget.background": "#1C1D21",
      "editorWidget.border": "#2A2C31",
      "editorSuggestWidget.background": "#1C1D21",
      "editorSuggestWidget.border": "#2A2C31",
      "editorSuggestWidget.selectedBackground": "#2A2D33",
      "scrollbarSlider.background": "#ffffff14",
      "scrollbarSlider.hoverBackground": "#ffffff22",
    },
  });
  monaco.editor.defineTheme("querylab-light", {
    base: "vs",
    inherit: true,
    rules: [
      { token: "keyword", foreground: "0F7B6C", fontStyle: "bold" },
      { token: "string", foreground: "A0661A" },
      { token: "number", foreground: "B4530F" },
      { token: "comment", foreground: "8A929C", fontStyle: "italic" },
      { token: "predefined", foreground: "2F5FD0" },
    ],
    colors: {
      "editor.background": "#FFFFFF",
      "editor.lineHighlightBackground": "#F5F7F9",
      "editor.lineHighlightBorder": "#00000000",
      "editorLineNumber.foreground": "#B4BAC2",
      "editorCursor.foreground": "#0F7B6C",
    },
  });
};

export function SqlEditor({ path, value, onChange, onRun, onSave, onFormat, onMount }: SqlEditorProps) {
  const { resolvedTheme } = useTheme();
  // Keep the latest callbacks in refs so Monaco actions (registered once) never go stale.
  const handlers = useRef({ onRun, onSave, onFormat });
  useEffect(() => {
    handlers.current = { onRun, onSave, onFormat };
  }, [onRun, onSave, onFormat]);

  const handleMount: OnMount = (editor, monaco) => {
    const runCurrent = () => handlers.current.onRun();

    editor.addAction({
      id: "querylab.run",
      label: "Run Query",
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter],
      contextMenuGroupId: "navigation",
      contextMenuOrder: 1,
      run: runCurrent,
    });
    editor.addAction({
      id: "querylab.save",
      label: "Save Query",
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS],
      run: () => handlers.current.onSave(),
    });
    editor.addAction({
      id: "querylab.format",
      label: "Format SQL",
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyF],
      contextMenuGroupId: "1_modification",
      run: () => handlers.current.onFormat(),
    });

    editor.focus();
    onMount?.({ editor, monaco });
  };

  return (
    <Editor
      path={path}
      language="sql"
      value={value}
      theme={resolvedTheme === "light" ? "querylab-light" : "querylab-dark"}
      beforeMount={defineThemes}
      onMount={handleMount}
      onChange={(v) => onChange(v ?? "")}
      loading={<Skeleton className="m-3 h-24 w-[calc(100%-1.5rem)]" />}
      options={{
        fontFamily: "var(--font-geist-mono), ui-monospace, SFMono-Regular, Menlo, monospace",
        fontSize: 13,
        lineHeight: 20,
        fontLigatures: false,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        renderLineHighlight: "line",
        smoothScrolling: true,
        cursorBlinking: "smooth",
        tabSize: 2,
        insertSpaces: true,
        autoIndent: "full",
        matchBrackets: "always",
        bracketPairColorization: { enabled: true },
        automaticLayout: true,
        wordWrap: "off",
        padding: { top: 10, bottom: 10 },
        quickSuggestions: { other: true, comments: false, strings: false },
        suggestOnTriggerCharacters: true,
        fixedOverflowWidgets: true,
        scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10 },
      }}
    />
  );
}
