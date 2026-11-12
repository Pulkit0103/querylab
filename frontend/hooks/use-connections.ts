"use client";

import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, ApiError } from "@/lib/api-client";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { Connection, ConnectionInput } from "@/types/api";

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
  test: (id: string) => ["connections", id, "test"] as const,
};

export function useConnections() {
  return useQuery<Connection[], ApiError>({
    queryKey: connectionKeys.all,
    queryFn: ({ signal }) => api.get<Connection[]>("/api/connections", { signal }),
  });
}

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
    mutationFn: (body) => api.post<Connection>("/api/connections", body),
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
      qc.removeQueries({ queryKey: connectionKeys.test(updated.id) });
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

/** Tests a saved connection; the result is cached as its status. */
export function useTestSavedConnection() {
  const qc = useQueryClient();
  return useMutation<ConnectionTestResult, ApiError, string>({
    mutationFn: (id) => api.post<ConnectionTestResult>(`/api/connections/${id}/test`),
    onSuccess: (result, id) => {
      qc.setQueryData(connectionKeys.test(id), { ok: true, result });
      void qc.invalidateQueries({ queryKey: connectionKeys.all });
    },
    onError: (error, id) => {
      qc.setQueryData(connectionKeys.test(id), { ok: false, error: error.message });
    },
  });
}

export type ConnectionStatus = { ok: true; result: ConnectionTestResult } | { ok: false; error: string };

export function useConnectionStatus(id: string | null) {
  return useQuery<ConnectionStatus | null>({
    queryKey: connectionKeys.test(id ?? "none"),
    queryFn: () => null,
    enabled: false,
    staleTime: Infinity,
  });
}
