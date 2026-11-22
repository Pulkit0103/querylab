/** Pure helpers for the schema diff view: selection, grouping and scripts. */
import type { DiffResponse, ObjectType, SchemaChange } from "@/types/migrations";

export const OBJECT_LABEL: Record<ObjectType, string> = {
  schema: "Schemas",
  type: "Types",
  sequence: "Sequences",
  table: "Tables",
  column: "Columns",
  constraint: "Constraints",
  index: "Indexes",
  view: "Views",
  function: "Functions",
  trigger: "Triggers",
};

const GROUP_ORDER: ObjectType[] = [
  "schema",
  "type",
  "sequence",
  "table",
  "column",
  "constraint",
  "index",
  "view",
  "function",
  "trigger",
];

/** A change that only explains manual work (its SQL is a comment). */
export const isManual = (c: SchemaChange) => c.sql.trimStart().startsWith("--");

/** Safe changes are selected by default; destructive and manual ones are not. */
export function defaultSelection(changes: SchemaChange[]): Set<number> {
  return new Set(changes.flatMap((c, i) => (c.destructive || isManual(c) ? [] : [i])));
}

export interface ChangeGroup {
  type: ObjectType;
  label: string;
  /** Indices into `changes`, in script order. */
  items: number[];
}

export function groupChanges(changes: SchemaChange[]): ChangeGroup[] {
  return GROUP_ORDER.map((type) => ({
    type,
    label: OBJECT_LABEL[type],
    items: changes.flatMap((c, i) => (c.object_type === type ? [i] : [])),
  })).filter((g) => g.items.length > 0);
}

export function summarize(changes: SchemaChange[]) {
  return {
    create: changes.filter((c) => c.action === "create").length,
    alter: changes.filter((c) => c.action === "alter").length,
    drop: changes.filter((c) => c.action === "drop").length,
    destructive: changes.filter((c) => c.destructive).length,
  };
}

/** Up script of the selected changes, in dependency order. */
export function buildScript(diff: DiffResponse, selected: Set<number>): string {
  const body = diff.changes.filter((_, i) => selected.has(i)).map((c) => c.sql);
  return body.length === 0 ? "" : [diff.preamble, ...body].join("\n\n") + "\n";
}

/**
 * Down script: inverse statements of the selected changes in reverse order.
 * `missing` lists changes that cannot be undone automatically.
 */
export function buildDownScript(diff: DiffResponse, selected: Set<number>): { sql: string; missing: string[] } {
  const chosen = diff.changes.filter((_, i) => selected.has(i)).reverse();
  const missing = chosen.filter((c) => !c.down_sql).map((c) => c.object);
  const body = chosen.flatMap((c) => (c.down_sql ? [c.down_sql] : [`-- Not reversible: ${c.object}`]));
  return { sql: body.length === 0 ? "" : [diff.preamble, ...body].join("\n\n") + "\n", missing };
}

/** Default migration name for a diff. */
export function migrationName(diff: DiffResponse, sourceLabel: string): string {
  return `Sync ${diff.target_schema} with ${sourceLabel}`.slice(0, 200);
}
