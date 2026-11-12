"use client";

import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, ApiError } from "@/lib/api-client";
import type {
  Collection,
  HistoryPage,
  HistoryStatus,
  SavedQuery,
  SavedQueryInput,
  Stats,
} from "@/types/library";

export const libraryKeys = {
  history: (filters: object) => ["history", filters] as const,
  saved: (filters: object) => ["saved", filters] as const,
  collections: ["collections"] as const,
  stats: (days: number) => ["stats", days] as const,
};

// -- history ------------------------------------------------------------------

export function useHistory(filters: { q?: string; status?: HistoryStatus; connection_id?: string }) {
  return useInfiniteQuery<HistoryPage, ApiError>({
    queryKey: libraryKeys.history(filters),
    initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam, signal }) =>
      api.get<HistoryPage>("/api/history", {
        signal,
        query: { limit: 50, before: pageParam as string | undefined, ...filters },
      }),
    getNextPageParam: (last) => last.next_cursor ?? undefined,
  });
}

export function useDeleteHistory() {
  const qc = useQueryClient();
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/history/${id}`),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["history"] }),
  });
}

export function useClearHistory() {
  const qc = useQueryClient();
  return useMutation<{ deleted: number }, ApiError, void>({
    mutationFn: () => api.delete<{ deleted: number }>("/api/history"),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["history"] });
      void qc.invalidateQueries({ queryKey: ["stats"] });
    },
  });
}

// -- saved queries ------------------------------------------------------------

export function useSavedQueries(filters: { q?: string; collection_id?: string }) {
  return useQuery<SavedQuery[], ApiError>({
    queryKey: libraryKeys.saved(filters),
    queryFn: ({ signal }) => api.get<SavedQuery[]>("/api/queries", { signal, query: filters }),
  });
}

function invalidateSaved(qc: ReturnType<typeof useQueryClient>) {
  void qc.invalidateQueries({ queryKey: ["saved"] });
  void qc.invalidateQueries({ queryKey: libraryKeys.collections });
}

export function useCreateSavedQuery() {
  const qc = useQueryClient();
  return useMutation<SavedQuery, ApiError, SavedQueryInput>({
    mutationFn: (body) => api.post<SavedQuery>("/api/queries", body),
    onSuccess: () => invalidateSaved(qc),
  });
}

export function useUpdateSavedQuery() {
  const qc = useQueryClient();
  return useMutation<SavedQuery, ApiError, { id: string; body: Partial<SavedQueryInput> }>({
    mutationFn: ({ id, body }) => api.put<SavedQuery>(`/api/queries/${id}`, body),
    onSuccess: () => invalidateSaved(qc),
  });
}

export function useDeleteSavedQuery() {
  const qc = useQueryClient();
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/queries/${id}`),
    onSuccess: () => invalidateSaved(qc),
  });
}

// -- collections --------------------------------------------------------------

export function useCollections() {
  return useQuery<Collection[], ApiError>({
    queryKey: libraryKeys.collections,
    queryFn: ({ signal }) => api.get<Collection[]>("/api/collections", { signal }),
  });
}

export function useCreateCollection() {
  const qc = useQueryClient();
  return useMutation<Collection, ApiError, string>({
    mutationFn: (name) => api.post<Collection>("/api/collections", { name }),
    onSuccess: () => void qc.invalidateQueries({ queryKey: libraryKeys.collections }),
  });
}

export function useRenameCollection() {
  const qc = useQueryClient();
  return useMutation<Collection, ApiError, { id: string; name: string }>({
    mutationFn: ({ id, name }) => api.put<Collection>(`/api/collections/${id}`, { name }),
    onSuccess: () => void qc.invalidateQueries({ queryKey: libraryKeys.collections }),
  });
}

export function useDeleteCollection() {
  const qc = useQueryClient();
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/collections/${id}`),
    onSuccess: () => invalidateSaved(qc),
  });
}

// -- stats --------------------------------------------------------------------

export function useStats(days = 14) {
  return useQuery<Stats, ApiError>({
    queryKey: libraryKeys.stats(days),
    queryFn: ({ signal }) => api.get<Stats>("/api/stats", { signal, query: { days } }),
  });
}
