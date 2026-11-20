"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, ApiError } from "@/lib/api-client";
import type { LayoutResponse, Positions, SchemaGraph } from "@/types/diagram";

export const diagramKeys = {
  graph: (connectionId: string, schema: string, tables: string[] | null, includeViews: boolean) =>
    ["schema", connectionId, "graph", schema, tables?.join(",") ?? "*", includeViews] as const,
  layout: (connectionId: string, schema: string) => ["diagram-layout", connectionId, schema] as const,
};

const base = (connectionId: string) => `/api/connections/${connectionId}`;
const layoutPath = (connectionId: string, schema: string) =>
  `${base(connectionId)}/diagram-layouts/${encodeURIComponent(schema)}`;

/**
 * Tables, keys and relationships of a schema. `tables` null shows the whole
 * schema (the most connected 150 tables for large ones).
 */
export function useSchemaGraph(
  connectionId: string | null,
  schema: string | null,
  tables: string[] | null,
  includeViews: boolean,
) {
  return useQuery<SchemaGraph, ApiError>({
    queryKey: diagramKeys.graph(connectionId ?? "none", schema ?? "", tables, includeViews),
    queryFn: ({ signal }) =>
      api.get<SchemaGraph>(`${base(connectionId!)}/schema-graph`, {
        signal,
        query: {
          schema: schema!,
          tables: tables?.join(","),
          include_views: includeViews || undefined,
        },
      }),
    enabled: connectionId !== null && !!schema && (tables === null || tables.length > 0),
    staleTime: 5 * 60_000,
    placeholderData: (prev) => prev,
  });
}

export function useDiagramLayout(connectionId: string | null, schema: string | null) {
  return useQuery<LayoutResponse, ApiError>({
    queryKey: diagramKeys.layout(connectionId ?? "none", schema ?? ""),
    queryFn: ({ signal }) => api.get<LayoutResponse>(layoutPath(connectionId!, schema!), { signal }),
    enabled: connectionId !== null && !!schema,
    staleTime: Infinity,
  });
}

export function useSaveDiagramLayout(connectionId: string, schema: string) {
  const qc = useQueryClient();
  return useMutation<LayoutResponse, ApiError, Positions>({
    mutationFn: (positions) => api.put<LayoutResponse>(layoutPath(connectionId, schema), { positions }),
    onSuccess: (data) => qc.setQueryData(diagramKeys.layout(connectionId, schema), data),
  });
}

export function useResetDiagramLayout(connectionId: string, schema: string) {
  const qc = useQueryClient();
  return useMutation<void, ApiError, void>({
    mutationFn: () => api.delete<void>(layoutPath(connectionId, schema)),
    onSuccess: () =>
      qc.setQueryData<LayoutResponse>(diagramKeys.layout(connectionId, schema), { positions: {}, updated_at: null }),
  });
}
