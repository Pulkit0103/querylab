"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, ApiError } from "@/lib/api-client";
import type {
  ActivityResponse,
  BackendSignal,
  LocksResponse,
  SizesResponse,
  TopQueriesResponse,
  TopQueriesSort,
} from "@/types/monitoring";

const base = (connectionId: string) => `/api/connections/${connectionId}/monitor`;

export const monitorKeys = {
  all: (connectionId: string) => ["monitor", connectionId] as const,
  view: (connectionId: string, view: string, extra?: string) => ["monitor", connectionId, view, extra ?? ""] as const,
};

/**
 * One monitoring view. Refreshes every `intervalMs` while mounted and the
 * tab is visible (`false` turns auto-refresh off). Rate-limit errors are not
 * retried; the next tick tries again.
 */
function useMonitorView<T>(connectionId: string | null, view: string, path: string, intervalMs: number | false, extra?: string) {
  return useQuery<T, ApiError>({
    queryKey: monitorKeys.view(connectionId ?? "none", view, extra),
    queryFn: ({ signal }) => api.get<T>(`${base(connectionId!)}/${path}`, { signal }),
    enabled: connectionId !== null,
    refetchInterval: intervalMs,
    refetchIntervalInBackground: false,
    retry: false,
    placeholderData: (prev) => prev,
  });
}

export const useActivity = (connectionId: string | null, intervalMs: number | false) =>
  useMonitorView<ActivityResponse>(connectionId, "activity", "activity", intervalMs);

export const useLocks = (connectionId: string | null, intervalMs: number | false) =>
  useMonitorView<LocksResponse>(connectionId, "locks", "locks", intervalMs);

export const useSizes = (connectionId: string | null, schema: string | null) =>
  useMonitorView<SizesResponse>(
    connectionId,
    "sizes",
    schema ? `sizes?schema=${encodeURIComponent(schema)}` : "sizes",
    false,
    schema ?? "*",
  );

export const useTopQueries = (connectionId: string | null, sort: TopQueriesSort, intervalMs: number | false) =>
  useMonitorView<TopQueriesResponse>(connectionId, "top-queries", `top-queries?sort=${sort}`, intervalMs, sort);

export function useSignalBackend(connectionId: string) {
  const qc = useQueryClient();
  return useMutation<{ pid: number; signalled: boolean }, ApiError, { pid: number; signal: BackendSignal; confirm?: string }>({
    mutationFn: ({ pid, signal, confirm }) =>
      api.post(`${base(connectionId)}/backends/${pid}/${signal}`, { confirm }),
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: monitorKeys.all(connectionId) });
      void qc.invalidateQueries({ queryKey: ["history"] });
    },
  });
}
