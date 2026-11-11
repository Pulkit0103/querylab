/**
 * Phase 2 demo data. Lets the workspace UI be built and reviewed before the
 * schema explorer and execution engine are wired to the API. Removed once the
 * real endpoints land.
 */
import type { QueryResult } from "@/types/query";
import type { ColumnInfo, SchemaInfo, TableInfo } from "@/types/schema";

export const mockSchemas: SchemaInfo[] = [
  { name: "public", table_count: 2 },
  { name: "sales", table_count: 3 },
];

export const mockTables: TableInfo[] = [
  { schema: "public", name: "departments", kind: "table", estimated_rows: 5, total_bytes: 32768, comment: null },
  { schema: "public", name: "employees", kind: "table", estimated_rows: 200, total_bytes: 98304, comment: null },
  { schema: "sales", name: "customers", kind: "table", estimated_rows: 500, total_bytes: 81920, comment: null },
  { schema: "sales", name: "orders", kind: "table", estimated_rows: 10000, total_bytes: 1236992, comment: null },
  { schema: "sales", name: "revenue_by_country", kind: "view", estimated_rows: null, total_bytes: null, comment: null },
];

const col = (
  name: string,
  ordinal: number,
  data_type: string,
  extra: Partial<ColumnInfo> = {},
): ColumnInfo => ({
  name,
  ordinal,
  data_type,
  nullable: false,
  default_value: null,
  is_primary_key: false,
  foreign_key: null,
  comment: null,
  ...extra,
});

export const mockColumns: Record<string, ColumnInfo[]> = {
  "public.departments": [
    col("id", 1, "integer", { is_primary_key: true, default_value: "nextval('departments_id_seq')" }),
    col("name", 2, "text"),
    col("created_at", 3, "timestamp with time zone", { default_value: "now()" }),
  ],
  "public.employees": [
    col("id", 1, "integer", { is_primary_key: true }),
    col("department_id", 2, "integer", {
      foreign_key: { constraint: "employees_department_id_fkey", schema: "public", table: "departments", column: "id" },
    }),
    col("first_name", 3, "text"),
    col("last_name", 4, "text"),
    col("email", 5, "text"),
    col("salary", 6, "numeric(12,2)"),
    col("hired_on", 7, "date"),
    col("is_active", 8, "boolean", { default_value: "true" }),
  ],
  "sales.customers": [
    col("id", 1, "uuid", { is_primary_key: true }),
    col("name", 2, "text"),
    col("country", 3, "character(2)"),
  ],
  "sales.orders": [
    col("id", 1, "bigint", { is_primary_key: true }),
    col("customer_id", 2, "uuid", {
      foreign_key: { constraint: "orders_customer_id_fkey", schema: "sales", table: "customers", column: "id" },
    }),
    col("status", 3, "text"),
    col("total", 4, "numeric(12,2)"),
    col("ordered_at", 5, "timestamp with time zone"),
  ],
  "sales.revenue_by_country": [
    col("country", 1, "character(2)", { nullable: true }),
    col("orders", 2, "bigint", { nullable: true }),
    col("revenue", 3, "numeric", { nullable: true }),
  ],
};

const names = ["Ada Lovelace", "Grace Hopper", "Linus Torvalds", "Margaret Hamilton", "Ken Thompson", "Barbara Liskov"];

export async function mockExecute(sql: string, signal?: AbortSignal): Promise<QueryResult> {
  const started = performance.now();
  await new Promise<void>((resolve, reject) => {
    const t = setTimeout(resolve, 250 + Math.random() * 400);
    signal?.addEventListener("abort", () => {
      clearTimeout(t);
      reject(new DOMException("Aborted", "AbortError"));
    });
  });
  if (/\berror\b/i.test(sql)) {
    throw Object.assign(new Error('syntax error at or near "error"'), { code: "sql_error" });
  }
  const rows = Array.from({ length: 137 }, (_, i) => [
    i + 1,
    names[i % names.length],
    `employee${i + 1}@example.com`,
    Math.round(55000 + ((i * 7919) % 95000)),
    i % 7 !== 0,
    new Date(Date.UTC(2020, i % 12, 1 + (i % 28))).toISOString(),
  ]);
  return {
    status: "success",
    columns: [
      { name: "id", type: "int4" },
      { name: "name", type: "text" },
      { name: "email", type: "text" },
      { name: "salary", type: "numeric" },
      { name: "is_active", type: "bool" },
      { name: "hired_at", type: "timestamptz" },
    ],
    rows,
    row_count: rows.length,
    rows_affected: null,
    truncated: false,
    execution_time_ms: Math.round(performance.now() - started),
    command: "SELECT",
  };
}
