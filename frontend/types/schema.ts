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

export type ConstraintKind = "primary_key" | "foreign_key" | "unique" | "check" | "exclusion";

export interface ConstraintInfo {
  name: string;
  kind: ConstraintKind;
  columns: string[];
  definition: string;
  deferrable: boolean;
  referenced_table: string | null;
}

export interface TableDdl {
  schema: string;
  name: string;
  kind: TableKind;
  ddl: string;
}

export type FunctionKind = "function" | "procedure" | "aggregate" | "window";

export interface FunctionInfo {
  oid: number;
  schema: string;
  name: string;
  kind: FunctionKind;
  arguments: string;
  result: string | null;
  language: string;
  volatility: "immutable" | "stable" | "volatile";
  security_definer: boolean;
  comment: string | null;
}

export interface FunctionDetail extends FunctionInfo {
  definition: string | null;
}

export interface TriggerInfo {
  schema: string;
  table: string;
  name: string;
  timing: "BEFORE" | "AFTER" | "INSTEAD OF";
  events: string[];
  level: "ROW" | "STATEMENT";
  function: string;
  enabled: "enabled" | "disabled" | "replica" | "always";
  definition: string;
}

export interface SequenceInfo {
  schema: string;
  name: string;
  data_type: string;
  start_value: number;
  increment_by: number;
  last_value: number | null;
  owned_by: string | null;
}

export interface TypeInfo {
  schema: string;
  name: string;
  kind: "enum" | "domain" | "composite" | "range";
  detail: string | null;
  values: string[];
  comment: string | null;
}

export interface SchemaObjects {
  sequences: SequenceInfo[];
  types: TypeInfo[];
}

export type SearchHitKind = TableKind | "column" | "function";

export interface SearchHit {
  kind: SearchHitKind;
  schema: string;
  table: string | null;
  name: string;
  detail: string | null;
}
