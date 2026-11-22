"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, ApiError } from "@/lib/api-client";
import { isFinished } from "@/lib/data-jobs";
import type {
  CheckImportResponse,
  ColumnMapping,
  ImportPreview,
  JobKind,
  JobRecord,
  JobView,
  MockPreview,
  MockRequest,
  ParseOptions,
  RunImportRequest,
} from "@/types/data-jobs";

export const jobKeys = {
  list: (kind?: JobKind) => ["data-jobs", kind ?? "all"] as const,
  job: (id: string) => ["data-job", id] as const,
};

export function useCreateImport(connectionId: string | null) {
  return useMutation<{ import_id: string; max_chunk_bytes: number }, ApiError, { file_name: string; size: number }>({
    mutationFn: (body) => api.post(`/api/connections/${connectionId}/imports`, body),
  });
}

export function useAnalyzeImport() {
  return useMutation<ImportPreview, ApiError, { id: string; options: ParseOptions }>({
    mutationFn: ({ id, options }) => api.post<ImportPreview>(`/api/imports/${id}/analyze`, options),
  });
}

export function useCheckImport() {
  return useMutation<
    CheckImportResponse,
    ApiError,
    { id: string; schema: string; table: string; mapping?: ColumnMapping[] }
  >({
    mutationFn: ({ id, ...body }) => api.post<CheckImportResponse>(`/api/imports/${id}/check`, body),
  });
}

export function useRunImport() {
  const qc = useQueryClient();
  return useMutation<JobView, ApiError, { id: string; body: RunImportRequest }>({
    mutationFn: ({ id, body }) => api.post<JobView>(`/api/imports/${id}/run`, body),
    onSuccess: (view) => qc.setQueryData(jobKeys.job(view.id), view),
  });
}

export function useDiscardImport() {
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/imports/${id}`),
  });
}

/** A job, polled every second until it finishes. */
export function useJob(id: string | null) {
  const qc = useQueryClient();
  return useQuery<JobView, ApiError>({
    queryKey: jobKeys.job(id ?? "none"),
    queryFn: async ({ signal }) => {
      const view = await api.get<JobView>(`/api/data-jobs/${id}`, { signal });
      if (isFinished(view.status)) {
        void qc.invalidateQueries({ queryKey: ["data-jobs"] });
        void qc.invalidateQueries({ queryKey: ["history"] });
      }
      return view;
    },
    enabled: id !== null,
    refetchInterval: (q) => (q.state.data && isFinished(q.state.data.status) ? false : 1000),
    retry: false,
  });
}

export function useCancelJob() {
  const qc = useQueryClient();
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.post<void>(`/api/data-jobs/${id}/cancel`),
    onSuccess: (_, id) => void qc.invalidateQueries({ queryKey: jobKeys.job(id) }),
  });
}

export function useJobs(kind?: JobKind) {
  return useQuery<JobRecord[], ApiError>({
    queryKey: jobKeys.list(kind),
    queryFn: ({ signal }) => api.get<JobRecord[]>("/api/data-jobs", { signal, query: { kind, limit: 50 } }),
  });
}

export function useMockPreview(connectionId: string | null) {
  return useMutation<MockPreview, ApiError, MockRequest>({
    mutationFn: (body) => api.post<MockPreview>(`/api/connections/${connectionId}/mock-data/preview`, body),
  });
}

export function useMockRun(connectionId: string | null) {
  const qc = useQueryClient();
  return useMutation<JobView, ApiError, MockRequest>({
    mutationFn: (body) => api.post<JobView>(`/api/connections/${connectionId}/mock-data/run`, body),
    onSuccess: (view) => qc.setQueryData(jobKeys.job(view.id), view),
  });
}
