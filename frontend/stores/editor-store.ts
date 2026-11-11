"use client";

import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

import type { ExplainResult, QueryResult } from "@/types/query";

export type ResultsView = "results" | "messages" | "plan";

export interface ExecError {
  code: string;
  message: string;
  details?: unknown;
}

export type ExecState =
  | { status: "idle" }
  | { status: "running"; kind: "execute" | "explain"; executionId: string; startedAt: number }
  | { status: "success"; result: QueryResult; finishedAt: number }
  | { status: "error"; error: ExecError; finishedAt: number };

export interface Message {
  id: string;
  level: "info" | "success" | "warning" | "error";
  text: string;
  detail?: string;
  at: number;
}

export interface EditorTab {
  id: string;
  title: string;
  sql: string;
  /** Set when the tab is bound to a saved query. */
  savedQueryId: string | null;
  /** SQL content at last save; used to compute the dirty indicator. */
  savedSql: string | null;
}

export interface TabRuntime {
  exec: ExecState;
  plan: ExplainResult | null;
  planError: ExecError | null;
  messages: Message[];
  view: ResultsView;
}

const MAX_MESSAGES = 100;

function newId(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : Math.random().toString(36).slice(2);
}

function emptyRuntime(): TabRuntime {
  return { exec: { status: "idle" }, plan: null, planError: null, messages: [], view: "results" };
}

const DEFAULT_SQL = `-- Welcome to QueryLab. Press Ctrl+Enter to run.
SELECT now() AS server_time, current_user, version();
`;

interface EditorState {
  tabs: EditorTab[];
  activeTabId: string;
  runtime: Record<string, TabRuntime>;
  untitledCounter: number;

  newTab: (init?: Partial<Pick<EditorTab, "title" | "sql" | "savedQueryId">>) => string;
  openSavedQuery: (q: { id: string; name: string; sql: string }) => void;
  closeTab: (id: string) => void;
  setActiveTab: (id: string) => void;
  updateSql: (id: string, sql: string) => void;
  renameTab: (id: string, title: string) => void;
  markSaved: (id: string, savedQueryId: string, title: string) => void;

  runtimeFor: (id: string) => TabRuntime;
  startExec: (id: string, kind: "execute" | "explain", executionId: string) => void;
  finishExec: (id: string, result: QueryResult) => void;
  failExec: (id: string, error: ExecError) => void;
  finishExplain: (id: string, plan: ExplainResult) => void;
  failExplain: (id: string, error: ExecError) => void;
  addMessage: (id: string, msg: Omit<Message, "id" | "at">) => void;
  clearMessages: (id: string) => void;
  setView: (id: string, view: ResultsView) => void;
}

function initialTab(): EditorTab {
  return { id: newId(), title: "Query 1", sql: DEFAULT_SQL, savedQueryId: null, savedSql: null };
}

export const useEditorStore = create<EditorState>()(
  persist(
    (set, get) => {
      const updateRuntime = (id: string, fn: (rt: TabRuntime) => TabRuntime) =>
        set((s) => ({ runtime: { ...s.runtime, [id]: fn(s.runtime[id] ?? emptyRuntime()) } }));

      const first = initialTab();
      return {
        tabs: [first],
        activeTabId: first.id,
        runtime: {},
        untitledCounter: 1,

        newTab: (init) => {
          const counter = get().untitledCounter + 1;
          const tab: EditorTab = {
            id: newId(),
            title: init?.title ?? `Query ${counter}`,
            sql: init?.sql ?? "",
            savedQueryId: init?.savedQueryId ?? null,
            savedSql: init?.savedQueryId ? (init.sql ?? "") : null,
          };
          set((s) => ({ tabs: [...s.tabs, tab], activeTabId: tab.id, untitledCounter: counter }));
          return tab.id;
        },

        openSavedQuery: (q) => {
          const existing = get().tabs.find((t) => t.savedQueryId === q.id);
          if (existing) {
            set({ activeTabId: existing.id });
            return;
          }
          get().newTab({ title: q.name, sql: q.sql, savedQueryId: q.id });
        },

        closeTab: (id) =>
          set((s) => {
            const idx = s.tabs.findIndex((t) => t.id === id);
            if (idx === -1) return s;
            const remaining = s.tabs.filter((t) => t.id !== id);
            const runtime = { ...s.runtime };
            delete runtime[id];
            if (remaining.length === 0) {
              const fresh = { ...initialTab(), sql: "" };
              return { tabs: [fresh], activeTabId: fresh.id, runtime, untitledCounter: 1 };
            }
            const activeTabId =
              s.activeTabId === id ? remaining[Math.min(idx, remaining.length - 1)].id : s.activeTabId;
            return { tabs: remaining, activeTabId, runtime };
          }),

        setActiveTab: (id) => set({ activeTabId: id }),

        updateSql: (id, sql) =>
          set((s) => ({ tabs: s.tabs.map((t) => (t.id === id ? { ...t, sql } : t)) })),

        renameTab: (id, title) =>
          set((s) => ({ tabs: s.tabs.map((t) => (t.id === id ? { ...t, title } : t)) })),

        markSaved: (id, savedQueryId, title) =>
          set((s) => ({
            tabs: s.tabs.map((t) => (t.id === id ? { ...t, savedQueryId, title, savedSql: t.sql } : t)),
          })),

        runtimeFor: (id) => get().runtime[id] ?? emptyRuntime(),

        startExec: (id, kind, executionId) =>
          updateRuntime(id, (rt) => ({
            ...rt,
            exec: { status: "running", kind, executionId, startedAt: Date.now() },
          })),

        finishExec: (id, result) =>
          updateRuntime(id, (rt) => ({
            ...rt,
            exec: { status: "success", result, finishedAt: Date.now() },
            view: "results",
          })),

        failExec: (id, error) =>
          updateRuntime(id, (rt) => ({
            ...rt,
            exec: { status: "error", error, finishedAt: Date.now() },
            view: "messages",
          })),

        finishExplain: (id, plan) =>
          updateRuntime(id, (rt) => ({
            ...rt,
            exec: rt.exec.status === "running" ? { status: "idle" } : rt.exec,
            plan,
            planError: null,
            view: "plan",
          })),

        failExplain: (id, error) =>
          updateRuntime(id, (rt) => ({
            ...rt,
            exec: rt.exec.status === "running" ? { status: "idle" } : rt.exec,
            planError: error,
            view: "messages",
          })),

        addMessage: (id, msg) =>
          updateRuntime(id, (rt) => ({
            ...rt,
            messages: [...rt.messages, { ...msg, id: newId(), at: Date.now() }].slice(-MAX_MESSAGES),
          })),

        clearMessages: (id) => updateRuntime(id, (rt) => ({ ...rt, messages: [] })),

        setView: (id, view) => updateRuntime(id, (rt) => ({ ...rt, view })),
      };
    },
    {
      name: "querylab.editor",
      version: 1,
      storage: createJSONStorage(() => localStorage),
      // Only persist editor content; results are transient and may be large.
      partialize: (s) => ({ tabs: s.tabs, activeTabId: s.activeTabId, untitledCounter: s.untitledCounter }),
    },
  ),
);

export function isTabDirty(tab: EditorTab): boolean {
  return tab.savedQueryId !== null && tab.savedSql !== tab.sql;
}
