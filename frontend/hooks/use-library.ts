"use client";

import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, ApiError } from "@/lib/api-client";
import type {
  Collection,
  HistoryEntry,
  HistoryFilters,
  HistoryPage,
  SavedQuery,
  SavedQueryInput,
  Stats,
  TagCount,
  WorkspaceSettings,
} from "@/types/library";

export const libraryKeys = {
  history: (filters: object) => ["history", filters] as const,
  saved: (filters: object) => ["saved", filters] as const,
  collections: ["collections"] as const,
  tags: ["tags"] as const,
  stats: (days: number) => ["stats", days] as const,
  settings: ["workspace-settings"] as const,
};

// -- history ------------------------------------------------------------------

export function useHistory(filters: HistoryFilters, enabled = true) {
  return useInfiniteQuery<HistoryPage, ApiError>({
    queryKey: libraryKeys.history(filters),
    initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam, signal }) =>
      api.get<HistoryPage>("/api/history", {
        signal,
        query: { limit: 50, before: pageParam as string | undefined, ...filters },
      }),
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    enabled,
  });
}

export function useDeleteHistory() {
  const qc = useQueryClient();
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/history/${id}`),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["history"] }),
  });
}

export function usePinHistory() {
  const qc = useQueryClient();
  return useMutation<HistoryEntry, ApiError, { id: string; pinned: boolean }>({
    mutationFn: ({ id, pinned }) => api.put<HistoryEntry>(`/api/history/${id}/pin`, { pinned }),
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

export interface SavedFilters {
  q?: string;
  collection_id?: string;
  /** Comma-separated; queries must carry every tag. */
  tags?: string;
  pinned?: boolean;
}

export function useSavedQueries(filters: SavedFilters, enabled = true) {
  return useQuery<SavedQuery[], ApiError>({
    queryKey: libraryKeys.saved(filters),
    queryFn: ({ signal }) => api.get<SavedQuery[]>("/api/queries", { signal, query: { ...filters } }),
    enabled,
  });
}

function invalidateSaved(qc: ReturnType<typeof useQueryClient>) {
  void qc.invalidateQueries({ queryKey: ["saved"] });
  void qc.invalidateQueries({ queryKey: libraryKeys.collections });
  void qc.invalidateQueries({ queryKey: libraryKeys.tags });
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

export function usePinSavedQuery() {
  const qc = useQueryClient();
  return useMutation<SavedQuery, ApiError, { id: string; pinned: boolean }>({
    mutationFn: ({ id, pinned }) => api.put<SavedQuery>(`/api/queries/${id}/pin`, { pinned }),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["saved"] }),
  });
}

export function useDeleteSavedQuery() {
  const qc = useQueryClient();
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/queries/${id}`),
    onSuccess: () => invalidateSaved(qc),
  });
}

export function useTags() {
  return useQuery<TagCount[], ApiError>({
    queryKey: libraryKeys.tags,
    queryFn: ({ signal }) => api.get<TagCount[]>("/api/tags", { signal }),
  });
}

// -- folders ------------------------------------------------------------------

export function useCollections() {
  return useQuery<Collection[], ApiError>({
    queryKey: libraryKeys.collections,
    queryFn: ({ signal }) => api.get<Collection[]>("/api/collections", { signal }),
  });
}

export function useCreateCollection() {
  const qc = useQueryClient();
  return useMutation<Collection, ApiError, { name: string; parent_id?: string | null }>({
    mutationFn: (body) => api.post<Collection>("/api/collections", body),
    onSuccess: () => void qc.invalidateQueries({ queryKey: libraryKeys.collections }),
  });
}

/** Rename (`name`) and/or move (`parent_id`, null = top level). */
export function useUpdateCollection() {
  const qc = useQueryClient();
  return useMutation<Collection, ApiError, { id: string; name?: string; parent_id?: string | null }>({
    mutationFn: ({ id, ...body }) => api.put<Collection>(`/api/collections/${id}`, body),
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

// -- stats and settings -------------------------------------------------------

export function useStats(days = 14) {
  return useQuery<Stats, ApiError>({
    queryKey: libraryKeys.stats(days),
    queryFn: ({ signal }) => api.get<Stats>("/api/stats", { signal, query: { days } }),
  });
}

export function useWorkspaceSettings() {
  return useQuery<WorkspaceSettings, ApiError>({
    queryKey: libraryKeys.settings,
    queryFn: ({ signal }) => api.get<WorkspaceSettings>("/api/workspace/settings", { signal }),
  });
}

export function useSaveWorkspaceSettings() {
  const qc = useQueryClient();
  return useMutation<WorkspaceSettings, ApiError, WorkspaceSettings>({
    mutationFn: (body) => api.put<WorkspaceSettings>("/api/workspace/settings", body),
    onSuccess: (data) => qc.setQueryData(libraryKeys.settings, data),
  });
}
