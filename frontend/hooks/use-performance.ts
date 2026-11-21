"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, ApiError } from "@/lib/api-client";
import type { ExplainResult } from "@/types/query";
import type { FingerprintStats, FingerprintTimeline, SavedPlan, SavedPlanSummary } from "@/types/performance";

export const performanceKeys = {
  fingerprints: (connectionId: string | null, days: number) => ["performance", "list", connectionId ?? "*", days] as const,
  timeline: (fingerprint: string, connectionId: string | null, days: number) =>
    ["performance", "timeline", fingerprint, connectionId ?? "*", days] as const,
  plans: ["plans"] as const,
  plan: (id: string) => ["plans", id] as const,
};

export function useFingerprints(connectionId: string | null, days: number) {
  return useQuery<FingerprintStats[], ApiError>({
    queryKey: performanceKeys.fingerprints(connectionId, days),
    queryFn: ({ signal }) =>
      api.get<FingerprintStats[]>("/api/performance/fingerprints", {
        signal,
        query: { days, connection_id: connectionId ?? undefined },
      }),
    placeholderData: (prev) => prev,
  });
}

export function useFingerprintTimeline(fingerprint: string | null, connectionId: string | null, days: number) {
  return useQuery<FingerprintTimeline, ApiError>({
    queryKey: performanceKeys.timeline(fingerprint ?? "", connectionId, days),
    queryFn: ({ signal }) =>
      api.get<FingerprintTimeline>(`/api/performance/fingerprints/${encodeURIComponent(fingerprint!)}`, {
        signal,
        query: { days, connection_id: connectionId ?? undefined },
      }),
    enabled: fingerprint !== null,
  });
}

export function useSavedPlans() {
  return useQuery<SavedPlanSummary[], ApiError>({
    queryKey: performanceKeys.plans,
    queryFn: ({ signal }) => api.get<SavedPlanSummary[]>("/api/plans", { signal }),
  });
}

export function useSavedPlan(id: string | null) {
  return useQuery<SavedPlan, ApiError>({
    queryKey: performanceKeys.plan(id ?? ""),
    queryFn: ({ signal }) => api.get<SavedPlan>(`/api/plans/${id}`, { signal }),
    enabled: id !== null,
    staleTime: Infinity,
  });
}

export function useSavePlan() {
  const qc = useQueryClient();
  return useMutation<
    SavedPlan,
    ApiError,
    { name: string; connection_id: string | null; sql: string; plan: ExplainResult }
  >({
    mutationFn: (body) => api.post<SavedPlan>("/api/plans", body),
    onSuccess: () => void qc.invalidateQueries({ queryKey: performanceKeys.plans }),
  });
}

export function useDeletePlan() {
  const qc = useQueryClient();
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/plans/${id}`),
    onSuccess: () => void qc.invalidateQueries({ queryKey: performanceKeys.plans }),
  });
}
