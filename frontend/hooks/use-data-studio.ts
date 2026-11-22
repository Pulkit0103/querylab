"use client";

import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, ApiError } from "@/lib/api-client";
import { postDownload } from "@/lib/download";
import type {
  ApplyResponse,
  BrowseRequest,
  BrowseResponse,
  ChangeSet,
  ExportFormat,
  FilterGroup,
  PreviewResponse,
  SortKey,
  ValueMap,
} from "@/types/data";

const base = (connectionId: string) => `/api/connections/${connectionId}/data`;

export const dataKeys = {
  all: (connectionId: string) => ["data", connectionId] as const,
  browse: (connectionId: string, req: BrowseRequest) => ["data", connectionId, "browse", req] as const,
  preview: (connectionId: string, body: unknown) => ["data", connectionId, "preview", body] as const,
};

/** One page of rows. The previous page stays visible while the next loads. */
export function useBrowse(connectionId: string | null, req: BrowseRequest | null) {
  return useQuery<BrowseResponse, ApiError>({
    queryKey: dataKeys.browse(connectionId ?? "none", req ?? { schema: "", table: "" }),
    queryFn: ({ signal }) => api.post<BrowseResponse>(`${base(connectionId!)}/browse`, req, { signal }),
    enabled: connectionId !== null && req !== null,
    placeholderData: keepPreviousData,
    retry: false,
    refetchOnWindowFocus: false,
  });
}

export interface PreviewBody {
  schema: string;
  table: string;
  filter?: FilterGroup | null;
  sort?: SortKey[];
  search?: string;
  page_size?: number;
  changes?: ChangeSet;
}

/** Server-rendered SQL for a filter or change set (never executed). */
export function usePreview(connectionId: string | null, body: PreviewBody | null) {
  return useQuery<PreviewResponse, ApiError>({
    queryKey: dataKeys.preview(connectionId ?? "none", body),
    queryFn: ({ signal }) => api.post<PreviewResponse>(`${base(connectionId!)}/preview`, body, { signal }),
    enabled: connectionId !== null && body !== null,
    placeholderData: keepPreviousData,
    retry: false,
    staleTime: 60_000,
  });
}

export interface ApplyBody {
  schema: string;
  table: string;
  changes: ChangeSet;
  confirm?: string;
}

export function useApplyChanges(connectionId: string | null) {
  const qc = useQueryClient();
  return useMutation<ApplyResponse, ApiError, ApplyBody>({
    mutationFn: (body) => api.post<ApplyResponse>(`${base(connectionId!)}/apply`, body),
    onSuccess: (result) => {
      if (result.committed && connectionId) {
        void qc.invalidateQueries({ queryKey: ["data", connectionId, "browse"] });
        void qc.invalidateQueries({ queryKey: ["history"] });
      }
    },
  });
}

export interface ExportBody {
  schema: string;
  table: string;
  format: ExportFormat;
  filter?: FilterGroup | null;
  sort?: SortKey[];
  search?: string;
  keys?: ValueMap[];
  columns?: string[];
}

/** Streams an export from the server and saves it as a file. */
export function downloadExport(connectionId: string, body: ExportBody): Promise<void> {
  return postDownload(`${base(connectionId)}/export`, body, `${body.table}.${body.format}`);
}
