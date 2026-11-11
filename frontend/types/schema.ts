/** Schema explorer contract (mirrors backend `models::schema`). */

export interface SchemaInfo {
  name: string;
  table_count: number;
}

export type TableKind = "table" | "view" | "materialized_view" | "foreign_table" | "partitioned_table";

export interface TableInfo {
  schema: string;
  name: string;
  kind: TableKind;
  estimated_rows: number | null;
  total_bytes: number | null;
  comment: string | null;
}

export interface ForeignKeyRef {
  constraint: string;
  schema: string;
  table: string;
  column: string;
}

export interface ColumnInfo {
  name: string;
  ordinal: number;
  data_type: string;
  nullable: boolean;
  default_value: string | null;
  is_primary_key: boolean;
  foreign_key: ForeignKeyRef | null;
  comment: string | null;
}

export interface IndexInfo {
  name: string;
  definition: string;
  is_unique: boolean;
  is_primary: boolean;
}

export interface Relationship {
  constraint: string;
  direction: "outgoing" | "incoming";
  from_schema: string;
  from_table: string;
  from_columns: string[];
  to_schema: string;
  to_table: string;
  to_columns: string[];
}

export interface TableDetails {
  table: TableInfo;
  columns: ColumnInfo[];
  indexes: IndexInfo[];
}
