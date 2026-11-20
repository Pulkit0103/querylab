"use client";

import { useQuery } from "@tanstack/react-query";

import { api, ApiError } from "@/lib/api-client";
import type {
  ColumnInfo,
  ConstraintInfo,
  FunctionDetail,
  FunctionInfo,
  IndexInfo,
  Relationship,
  SchemaInfo,
  SchemaObjects,
  SearchHit,
  TableDdl,
  TableInfo,
  TriggerInfo,
} from "@/types/schema";

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
  functions: (connectionId: string) => ["schema", connectionId, "functions"] as const,
  func: (connectionId: string, oid: number) => ["schema", connectionId, "function", oid] as const,
  objects: (connectionId: string) => ["schema", connectionId, "objects"] as const,
  search: (connectionId: string, q: string) => ["schema", connectionId, "search", q] as const,
  table: (connectionId: string, schema: string, table: string, part: string) =>
    ["schema", connectionId, "table", schema, table, part] as const,
};

const base = (connectionId: string) => `/api/connections/${connectionId}`;
export const tablePath = (connectionId: string, table: string, part: string) =>
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

export function useConstraints(connectionId: string | null, schema: string, table: string, enabled = true) {
  return useQuery<ConstraintInfo[], ApiError>({
    queryKey: schemaKeys.table(connectionId ?? "none", schema, table, "constraints"),
    queryFn: ({ signal }) =>
      api.get<ConstraintInfo[]>(tablePath(connectionId!, table, "constraints"), { signal, query: { schema } }),
    enabled: enabled && connectionId !== null && table !== "",
    staleTime: SCHEMA_STALE_MS,
  });
}

export function ddlQuery(connectionId: string, schema: string, table: string) {
  return {
    queryKey: schemaKeys.table(connectionId, schema, table, "ddl"),
    queryFn: ({ signal }: { signal?: AbortSignal }) =>
      api.get<TableDdl>(tablePath(connectionId, table, "ddl"), { signal, query: { schema } }),
    staleTime: SCHEMA_STALE_MS,
  };
}

export function columnsQuery(connectionId: string, schema: string, table: string) {
  return {
    queryKey: schemaKeys.table(connectionId, schema, table, "columns"),
    queryFn: ({ signal }: { signal?: AbortSignal }) =>
      api.get<ColumnInfo[]>(tablePath(connectionId, table, "columns"), { signal, query: { schema } }),
    staleTime: SCHEMA_STALE_MS,
  };
}

export function useTableDdl(connectionId: string | null, schema: string, table: string, enabled = true) {
  return useQuery<TableDdl, ApiError>({
    ...ddlQuery(connectionId ?? "none", schema, table),
    enabled: enabled && connectionId !== null && table !== "",
  });
}

export function useTriggers(connectionId: string | null, schema: string, table: string, enabled = true) {
  return useQuery<TriggerInfo[], ApiError>({
    queryKey: schemaKeys.table(connectionId ?? "none", schema, table, "triggers"),
    queryFn: ({ signal }) =>
      api.get<TriggerInfo[]>(`${base(connectionId!)}/triggers`, { signal, query: { schema, table } }),
    enabled: enabled && connectionId !== null && table !== "",
    staleTime: SCHEMA_STALE_MS,
  });
}

export function functionsQuery(connectionId: string) {
  return {
    queryKey: schemaKeys.functions(connectionId),
    queryFn: ({ signal }: { signal?: AbortSignal }) =>
      api.get<FunctionInfo[]>(`${base(connectionId)}/functions`, { signal }),
    staleTime: SCHEMA_STALE_MS,
  };
}

/** Functions and procedures in every user schema (one request, grouped client-side). */
export function useFunctions(connectionId: string | null, enabled = true) {
  return useQuery<FunctionInfo[], ApiError>({
    ...functionsQuery(connectionId ?? "none"),
    enabled: enabled && connectionId !== null,
  });
}

export function useFunctionDetail(connectionId: string | null, oid: number | null) {
  return useQuery<FunctionDetail, ApiError>({
    queryKey: schemaKeys.func(connectionId ?? "none", oid ?? 0),
    queryFn: ({ signal }) => api.get<FunctionDetail>(`${base(connectionId!)}/functions/${oid}`, { signal }),
    enabled: connectionId !== null && oid !== null,
    staleTime: SCHEMA_STALE_MS,
  });
}

/** Sequences and user-defined types in every user schema. */
export function useSchemaObjects(connectionId: string | null, enabled = true) {
  return useQuery<SchemaObjects, ApiError>({
    queryKey: schemaKeys.objects(connectionId ?? "none"),
    queryFn: ({ signal }) => api.get<SchemaObjects>(`${base(connectionId!)}/objects`, { signal }),
    enabled: enabled && connectionId !== null,
    staleTime: SCHEMA_STALE_MS,
  });
}

/** Server-side search over tables, columns and functions (`q` ≥ 2 chars). */
export function useSchemaSearch(connectionId: string | null, q: string) {
  const query = q.trim();
  return useQuery<SearchHit[], ApiError>({
    queryKey: schemaKeys.search(connectionId ?? "none", query.toLowerCase()),
    queryFn: ({ signal }) => api.get<SearchHit[]>(`${base(connectionId!)}/search`, { signal, query: { q: query } }),
    enabled: connectionId !== null && query.length >= 2,
    staleTime: SCHEMA_STALE_MS,
    placeholderData: (prev) => prev,
  });
}