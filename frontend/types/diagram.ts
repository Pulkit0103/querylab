/** ER diagram graph and saved layouts (mirrors `models::diagram`). */

export interface GraphColumn {
  name: string;
  data_type: string;
  nullable: boolean;
  primary_key: boolean;
  foreign_key: boolean;
  unique: boolean;
}

export interface GraphTable {
  name: string;
  kind: "table" | "partitioned_table" | "foreign_table" | "view" | "materialized_view";
  estimated_rows: number | null;
  comment: string | null;
  columns: GraphColumn[];
}

export type Cardinality = "one_to_one" | "one_to_many";

export type ReferentialAction = "no_action" | "restrict" | "cascade" | "set_null" | "set_default";

/** A foreign key from `from_table` (child) to `to_table` (parent). */
export interface GraphEdge {
  constraint: string;
  from_table: string;
  from_columns: string[];
  to_table: string;
  to_columns: string[];
  cardinality: Cardinality;
  /** A nullable foreign key column: the child may reference nothing. */
  optional: boolean;
  on_delete: ReferentialAction;
  on_update: ReferentialAction;
}

export interface SchemaGraph {
  schema: string;
  tables: GraphTable[];
  edges: GraphEdge[];
  total_tables: number;
  truncated: boolean;
  hidden_edges: number;
}

export interface NodePosition {
  x: number;
  y: number;
}

export type Positions = Record<string, NodePosition>;

export interface LayoutResponse {
  positions: Positions;
  updated_at: string | null;
}
