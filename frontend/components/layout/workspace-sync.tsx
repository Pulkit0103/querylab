"use client";

import { useEffect, useRef } from "react";

import { useMe } from "@/hooks/use-auth";
import { api, ApiError } from "@/lib/api-client";
import { fromSession, restoreAction, toSession } from "@/lib/library";
import { useEditorStore } from "@/stores/editor-store";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { SessionResponse } from "@/types/library";

/** Edits are uploaded after this much quiet time. */
const SAVE_DELAY_MS = 1500;

/**
 * Keeps the open editor tabs in sync with the server so they follow the
 * user to other browsers. Local storage stays the offline cache: if the
 * server is unreachable, the editor keeps working on local tabs.
 *
 * On sign-in the newer side wins. While open, changes are saved after a
 * short delay (last write wins across browsers).
 */
export function WorkspaceSync() {
  const me = useMe();
  const userId = me.data?.id ?? null;
  const ready = useRef<string | null>(null);

  useEffect(() => {
    if (!userId) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let warned = false;

    const push = async () => {
      const editor = useEditorStore.getState();
      const session = toSession(editor.tabs, editor.activeTabId, useWorkspaceStore.getState().activeConnectionId);
      try {
        const saved = await api.put<SessionResponse>("/api/workspace/tabs", session);
        if (!cancelled && saved.updated_at) useEditorStore.getState().markSynced(userId, saved.updated_at);
      } catch (e) {
        // Offline or too large: keep working locally, report once.
        if (!warned && e instanceof ApiError && e.status !== 0) {
          warned = true;
          console.warn(`Tabs were not synced: ${e.message}`);
        }
      }
    };
    const schedule = () => {
      if (ready.current !== userId) return;
      clearTimeout(timer);
      timer = setTimeout(() => void push(), SAVE_DELAY_MS);
    };

    void (async () => {
      try {
        const server = await api.get<SessionResponse>("/api/workspace/tabs");
        if (cancelled) return;
        const editor = useEditorStore.getState();
        const action = restoreAction(userId, { owner: editor.syncOwner, syncedAt: editor.syncedAt }, server);
        if (action === "adopt" && server.session && server.updated_at) {
          const restored = fromSession(server.session);
          editor.replaceTabs(restored.tabs, restored.activeTabId);
          if (server.session.active_connection_id) {
            useWorkspaceStore.getState().setActiveConnection(server.session.active_connection_id);
          }
          editor.markSynced(userId, server.updated_at);
          ready.current = userId;
          return;
        }
        if (action === "reset") editor.resetTabs();
        ready.current = userId;
        await push();
      } catch {
        // Server unavailable: stay on local tabs and keep trying on edits.
        ready.current = userId;
      }
    })();

    const unsubEditor = useEditorStore.subscribe((s, prev) => {
      if (s.tabs !== prev.tabs || s.activeTabId !== prev.activeTabId) schedule();
    });
    const unsubWorkspace = useWorkspaceStore.subscribe((s, prev) => {
      if (s.activeConnectionId !== prev.activeConnectionId) schedule();
    });
    return () => {
      cancelled = true;
      clearTimeout(timer);
      unsubEditor();
      unsubWorkspace();
    };
  }, [userId]);

  return null;
}
