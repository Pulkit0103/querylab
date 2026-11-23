"use client";

import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, ApiError } from "@/lib/api-client";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { Connection, ConnectionGroup, ConnectionInput } from "@/types/api";

export interface ConnectionTestResult {
  ok: boolean;
  latency_ms: number;
  server_version: string;
  database: string;
  user: string;
  is_superuser: boolean;
  read_only: boolean;
  warnings: string[];
}

export const connectionKeys = {
  all: ["connections"] as const,
  groups: ["connection-groups"] as const,
};

/** Connections of the active workspace (all visible ones until it is known). */
export function useConnections() {
  const workspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  return useQuery<Connection[], ApiError, Connection[]>({
    queryKey: connectionKeys.all,
    queryFn: ({ signal }) => api.get<Connection[]>("/api/connections", { signal }),
    select: (list) => (workspaceId ? list.filter((c) => c.workspace_id === workspaceId) : list),
  });
}

const activeWorkspace = () => useWorkspaceStore.getState().activeWorkspaceId ?? undefined;

/**
 * The connection the workspace operates on. Falls back to the first
 * connection when nothing (or a deleted connection) is selected.
 */
export function useActiveConnection() {
  const { data: connections, isPending } = useConnections();
  const activeId = useWorkspaceStore((s) => s.activeConnectionId);
  const setActive = useWorkspaceStore((s) => s.setActiveConnection);

  const active = connections?.find((c) => c.id === activeId) ?? connections?.[0] ?? null;

  useEffect(() => {
    if (!isPending && active?.id !== activeId) setActive(active?.id ?? null);
  }, [isPending, active?.id, activeId, setActive]);

  return { connection: active, connections: connections ?? [], isPending };
}

export function useCreateConnection() {
  const qc = useQueryClient();
  const setActive = useWorkspaceStore((s) => s.setActiveConnection);
  return useMutation<Connection, ApiError, ConnectionInput>({
    mutationFn: (body) => api.post<Connection>("/api/connections", { workspace_id: activeWorkspace(), ...body }),
    onSuccess: (created) => {
      qc.setQueryData<Connection[]>(connectionKeys.all, (old) =>
        [...(old ?? []), created].sort((a, b) => a.name.localeCompare(b.name)),
      );
      setActive(created.id);
    },
  });
}

export function useUpdateConnection() {
  const qc = useQueryClient();
  return useMutation<Connection, ApiError, { id: string; body: Partial<ConnectionInput> }>({
    mutationFn: ({ id, body }) => api.put<Connection>(`/api/connections/${id}`, body),
    onSuccess: (updated) => {
      qc.setQueryData<Connection[]>(connectionKeys.all, (old) =>
        (old ?? []).map((c) => (c.id === updated.id ? updated : c)),
      );
      // Credentials or target may have changed; cached schema is stale.
      void qc.invalidateQueries({ queryKey: ["schema", updated.id] });
      void qc.invalidateQueries({ queryKey: connectionKeys.groups });
    },
  });
}

export function useDeleteConnection() {
  const qc = useQueryClient();
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/connections/${id}`),
    onSuccess: (_, id) => {
      qc.setQueryData<Connection[]>(connectionKeys.all, (old) => (old ?? []).filter((c) => c.id !== id));
      qc.removeQueries({ queryKey: ["schema", id] });
    },
  });
}

/** Tests an unsaved connection draft. */
export function useTestDraftConnection() {
  return useMutation<ConnectionTestResult, ApiError, ConnectionInput>({
    mutationFn: (body) => api.post<ConnectionTestResult>("/api/connections/test", body),
  });
}

/**
 * Tests a saved connection. The server records the outcome as the
 * connection's status (also on failure), so the list is refetched either way.
 */
export function useTestSavedConnection() {
  const qc = useQueryClient();
  return useMutation<ConnectionTestResult, ApiError, string>({
    mutationFn: (id) => api.post<ConnectionTestResult>(`/api/connections/${id}/test`),
    onSettled: () => void qc.invalidateQueries({ queryKey: connectionKeys.all }),
  });
}

/**
 * Reconnect: re-tests the connection and drops cached schema metadata so
 * the explorer and autocomplete reload from the server.
 */
export function useReconnect() {
  const qc = useQueryClient();
  return useMutation<ConnectionTestResult, ApiError, string>({
    mutationFn: (id) => api.post<ConnectionTestResult>(`/api/connections/${id}/test`),
    onSettled: (_, __, id) => {
      void qc.invalidateQueries({ queryKey: connectionKeys.all });
      void qc.invalidateQueries({ queryKey: ["schema", id] });
    },
  });
}

export function useDuplicateConnection() {
  const qc = useQueryClient();
  return useMutation<Connection, ApiError, string>({
    mutationFn: (id) => api.post<Connection>(`/api/connections/${id}/duplicate`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: connectionKeys.all });
      void qc.invalidateQueries({ queryKey: connectionKeys.groups });
    },
  });
}

// -- groups -----------------------------------------------------------------

export function useConnectionGroups() {
  const workspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  return useQuery<ConnectionGroup[], ApiError>({
    queryKey: [...connectionKeys.groups, workspaceId],
    queryFn: ({ signal }) =>
      api.get<ConnectionGroup[]>("/api/connection-groups", { signal, query: { workspace_id: workspaceId ?? undefined } }),
  });
}

export function useCreateGroup() {
  const qc = useQueryClient();
  return useMutation<ConnectionGroup, ApiError, { name: string }>({
    mutationFn: (body) => api.post<ConnectionGroup>("/api/connection-groups", { workspace_id: activeWorkspace(), ...body }),
    onSuccess: () => void qc.invalidateQueries({ queryKey: connectionKeys.groups }),
  });
}

export function useUpdateGroup() {
  const qc = useQueryClient();
  return useMutation<ConnectionGroup, ApiError, { id: string; name: string; position?: number }>({
    mutationFn: ({ id, ...body }) => api.put<ConnectionGroup>(`/api/connection-groups/${id}`, body),
    onSuccess: () => void qc.invalidateQueries({ queryKey: connectionKeys.groups }),
  });
}

export function useDeleteGroup() {
  const qc = useQueryClient();
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/connection-groups/${id}`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: connectionKeys.groups });
      // Its connections became ungrouped.
      void qc.invalidateQueries({ queryKey: connectionKeys.all });
    },
  });
}
