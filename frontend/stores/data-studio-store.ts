"use client";

import { create } from "zustand";

import { emptyChanges, type StagedChanges } from "@/lib/data-studio";

/**
 * Staged (unapplied) changes per table, kept in memory so switching tables
 * or pages does not lose them. Never persisted: a reload starts clean.
 */
interface DataStudioState {
  changes: Record<string, StagedChanges>;
  update: (tableKey: string, fn: (current: StagedChanges) => StagedChanges) => void;
  clear: (tableKey: string) => void;
}

export const studioTableKey = (connectionId: string, schema: string, table: string) =>
  JSON.stringify([connectionId, schema, table]);

export const useDataStudioStore = create<DataStudioState>()((set) => ({
  changes: {},
  update: (tableKey, fn) =>
    set((s) => ({ changes: { ...s.changes, [tableKey]: fn(s.changes[tableKey] ?? emptyChanges()) } })),
  clear: (tableKey) =>
    set((s) => {
      const changes = { ...s.changes };
      delete changes[tableKey];
      return { changes };
    }),
}));
