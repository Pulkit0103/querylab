"use client";

import { useQuery } from "@tanstack/react-query";

import { api, ApiError } from "@/lib/api-client";
import type { ColumnInfo, IndexInfo, Relationship, SchemaInfo, TableInfo } from "@/types/schema";

export interface CatalogEntry {
  schema: string;
  table: string;
  kind: TableInfo["kind"];
  columns: string[];
  column_types: string[];
}

export const schemaKeys = {
  all: (connectionId: string) => ["schema", connectionId] as const,
  schemas: (connectionId: string) => ["schema", connectionId, "schemas"] as const,
  tables: (connectionId: string) => ["schema", connectionId, "tables"] as const,
  catalog: (connectionId: string) => ["schema", connectionId, "catalog"] as const,
  table: (connectionId: string, schema: string, table: string, part: string) =>
    ["schema", connectionId, "table", schema, table, part] as const,
};

const base = (connectionId: string) => `/api/connections/${connectionId}`;
const tablePath = (connectionId: string, table: string, part: string) =>
  `${base(connectionId)}/tables/${encodeURIComponent(table)}/${part}`;

// Schema metadata changes rarely; keep it warm for a few minutes.
const SCHEMA_STALE_MS = 5 * 60_000;

export function useSchemas(connectionId: string | null) {
  return useQuery<SchemaInfo[], ApiError>({
    queryKey: schemaKeys.schemas(connectionId ?? "none"),
    queryFn: ({ signal }) => api.get<SchemaInfo[]>(`${base(connectionId!)}/schemas`, { signal }),
    enabled: connectionId !== null,
    staleTime: SCHEMA_STALE_MS,
  });
}

export function useTables(connectionId: string | null) {
  return useQuery<TableInfo[], ApiError>({
    queryKey: schemaKeys.tables(connectionId ?? "none"),
    queryFn: ({ signal }) => api.get<TableInfo[]>(`${base(connectionId!)}/tables`, { signal }),
    enabled: connectionId !== null,
    staleTime: SCHEMA_STALE_MS,
  });
}

export function useColumns(connectionId: string | null, schema: string, table: string, enabled = true) {
  return useQuery<ColumnInfo[], ApiError>({
    queryKey: schemaKeys.table(connectionId ?? "none", schema, table, "columns"),
    queryFn: ({ signal }) =>
      api.get<ColumnInfo[]>(tablePath(connectionId!, table, "columns"), { signal, query: { schema } }),
    enabled: enabled && connectionId !== null && table !== "",
    staleTime: SCHEMA_STALE_MS,
  });
}

export function useIndexes(connectionId: string | null, schema: string, table: string, enabled = true) {
  return useQuery<IndexInfo[], ApiError>({
    queryKey: schemaKeys.table(connectionId ?? "none", schema, table, "indexes"),
    queryFn: ({ signal }) =>
      api.get<IndexInfo[]>(tablePath(connectionId!, table, "indexes"), { signal, query: { schema } }),
    enabled: enabled && connectionId !== null && table !== "",
    staleTime: SCHEMA_STALE_MS,
  });
}

export function useRelationships(connectionId: string | null, schema: string, table: string, enabled = true) {
  return useQuery<Relationship[], ApiError>({
    queryKey: schemaKeys.table(connectionId ?? "none", schema, table, "relationships"),
    queryFn: ({ signal }) =>
      api.get<Relationship[]>(tablePath(connectionId!, table, "relationships"), { signal, query: { schema } }),
    enabled: enabled && connectionId !== null && table !== "",
    staleTime: SCHEMA_STALE_MS,
  });
}

/** Every table with its columns, used for editor autocomplete. */
export function useCatalog(connectionId: string | null) {
  return useQuery<CatalogEntry[], ApiError>({
    queryKey: schemaKeys.catalog(connectionId ?? "none"),
    queryFn: ({ signal }) => api.get<CatalogEntry[]>(`${base(connectionId!)}/catalog`, { signal }),
    enabled: connectionId !== null,
    staleTime: SCHEMA_STALE_MS,
  });
}
