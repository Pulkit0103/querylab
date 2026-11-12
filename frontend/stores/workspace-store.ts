"use client";

import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

import type { Connection } from "@/types/api";

interface WorkspaceState {
  /** Add/edit connection dialog; `editing` is null when adding. */
  connectionDialog: { open: boolean; editing: Connection | null };
  openConnectionDialog: (editing?: Connection | null) => void;
  closeConnectionDialog: () => void;

  /** Connection used by the editor and explorer. */
  activeConnectionId: string | null;
  sidebarCollapsed: boolean;
  /** Mobile navigation drawer. */
  mobileNavOpen: boolean;
  /** Maximum rows requested per execution. */
  maxRows: number;
  /** Explorer table currently shown in the details pane. */
  selectedTable: { schema: string; name: string } | null;

  setActiveConnection: (id: string | null) => void;
  toggleSidebar: () => void;
  setMobileNavOpen: (open: boolean) => void;
  setMaxRows: (n: number) => void;
  selectTable: (t: { schema: string; name: string } | null) => void;
}

export const useWorkspaceStore = create<WorkspaceState>()(
  persist(
    (set) => ({
      connectionDialog: { open: false, editing: null },
      openConnectionDialog: (editing = null) => set({ connectionDialog: { open: true, editing } }),
      closeConnectionDialog: () => set((s) => ({ connectionDialog: { ...s.connectionDialog, open: false } })),
      activeConnectionId: null,
      sidebarCollapsed: false,
      mobileNavOpen: false,
      maxRows: 1000,
      selectedTable: null,

      setActiveConnection: (id) => set({ activeConnectionId: id, selectedTable: null }),
      toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
      setMobileNavOpen: (open) => set({ mobileNavOpen: open }),
      setMaxRows: (n) => set({ maxRows: n }),
      selectTable: (t) => set({ selectedTable: t }),
    }),
    {
      name: "querylab.workspace",
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => ({
        activeConnectionId: s.activeConnectionId,
        sidebarCollapsed: s.sidebarCollapsed,
        maxRows: s.maxRows,
      }),
    },
  ),
);
