"use client";

import { useQuery } from "@tanstack/react-query";

import { mockColumns, mockSchemas, mockTables } from "@/lib/mock-data";
import type { ColumnInfo, SchemaInfo, TableInfo } from "@/types/schema";

export const schemaKeys = {
  all: (connectionId: string) => ["schema", connectionId] as const,
  schemas: (connectionId: string) => ["schema", connectionId, "schemas"] as const,
  tables: (connectionId: string) => ["schema", connectionId, "tables"] as const,
  columns: (connectionId: string, schema: string, table: string) =>
    ["schema", connectionId, "columns", schema, table] as const,
};

export function useSchemas(connectionId: string | null) {
  return useQuery<SchemaInfo[]>({
    queryKey: schemaKeys.schemas(connectionId ?? "none"),
    queryFn: async () => mockSchemas,
    enabled: connectionId !== null,
  });
}

export function useTables(connectionId: string | null) {
  return useQuery<TableInfo[]>({
    queryKey: schemaKeys.tables(connectionId ?? "none"),
    queryFn: async () => mockTables,
    enabled: connectionId !== null,
  });
}

export function useColumns(connectionId: string | null, schema: string, table: string, enabled = true) {
  return useQuery<ColumnInfo[]>({
    queryKey: schemaKeys.columns(connectionId ?? "none", schema, table),
    queryFn: async () => mockColumns[`${schema}.${table}`] ?? [],
    enabled: enabled && connectionId !== null,
  });
}
