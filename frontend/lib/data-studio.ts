/**
 * Pure Data Studio logic: staged change sets, the filter-builder model and
 * small helpers. Components and hooks stay thin wrappers around these.
 */
import type {
  ChangeKind,
  ChangeSet,
  ColumnKind,
  DataCell,
  DataColumn,
  FilterCondition,
  FilterGroup,
  FilterOperator,
  ValueMap,
} from "@/types/data";

/** URL of a table in the Table Editor. */
export function tableEditorHref(schema: string, table: string, path = "/tables"): string {
  return `${path}?${new URLSearchParams({ schema, table }).toString()}`;
}

let seq = 0;
export const nextId = (prefix: string) => `${prefix}${++seq}`;

// ---------------------------------------------------------------------------
// Staged changes
// ---------------------------------------------------------------------------

/** A loaded row identified by its key values. */
export interface RowRef {
  /** Stable id derived from the key values. */
  id: string;
  key: ValueMap;
}

export interface StagedEdit {
  key: ValueMap;
  /** New values of edited columns. */
  values: ValueMap;
  /** Values of the same columns when the row was loaded. */
  original: ValueMap;
}

export interface StagedInsert {
  id: string;
  /** Columns left out use their default. */
  values: ValueMap;
}

export interface StagedChanges {
  edits: Record<string, StagedEdit>;
  deletes: Record<string, ValueMap>;
  inserts: StagedInsert[];
}

export const emptyChanges = (): StagedChanges => ({ edits: {}, deletes: {}, inserts: [] });

/** Identifies a row by its key columns; null when the table has no key. */
export function rowRef(columns: DataColumn[], keyColumns: string[], row: DataCell[]): RowRef | null {
  if (keyColumns.length === 0) return null;
  const key: ValueMap = {};
  for (const name of keyColumns) {
    const index = columns.findIndex((c) => c.name === name);
    if (index < 0) return null;
    key[name] = row[index];
  }
  return { id: JSON.stringify(keyColumns.map((k) => key[k])), key };
}

/** Stages a cell edit; editing back to the original value un-stages it. */
export function stageEdit(
  state: StagedChanges,
  ref: RowRef,
  column: string,
  value: DataCell,
  original: DataCell,
): StagedChanges {
  const current = state.edits[ref.id] ?? { key: ref.key, values: {}, original: {} };
  const values = { ...current.values };
  const originals = { ...current.original };
  if (value === original) {
    delete values[column];
    delete originals[column];
  } else {
    values[column] = value;
    originals[column] = original;
  }
  const edits = { ...state.edits };
  if (Object.keys(values).length === 0) delete edits[ref.id];
  else edits[ref.id] = { key: ref.key, values, original: originals };
  return { ...state, edits };
}

/** Marks a row for deletion (dropping its edits), or un-marks it. */
export function toggleDelete(state: StagedChanges, ref: RowRef): StagedChanges {
  const deletes = { ...state.deletes };
  const edits = { ...state.edits };
  if (deletes[ref.id]) delete deletes[ref.id];
  else {
    deletes[ref.id] = ref.key;
    delete edits[ref.id];
  }
  return { ...state, deletes, edits };
}

export function addInsert(state: StagedChanges, id = nextId("new-")): StagedChanges {
  return { ...state, inserts: [{ id, values: {} }, ...state.inserts] };
}

/** `undefined` removes the value so the column default applies. */
export function setInsertValue(
  state: StagedChanges,
  id: string,
  column: string,
  value: DataCell | undefined,
): StagedChanges {
  return {
    ...state,
    inserts: state.inserts.map((row) => {
      if (row.id !== id) return row;
      const values = { ...row.values };
      if (value === undefined) delete values[column];
      else values[column] = value;
      return { ...row, values };
    }),
  };
}

export function removeInsert(state: StagedChanges, id: string): StagedChanges {
  return { ...state, inserts: state.inserts.filter((r) => r.id !== id) };
}

export function changeCount(state: StagedChanges): number {
  return Object.keys(state.edits).length + Object.keys(state.deletes).length + state.inserts.length;
}

/** The API payload. Original values enable the server's stale-write check. */
export function toChangeSet(state: StagedChanges): ChangeSet {
  return {
    inserts: state.inserts.map((r) => ({ values: r.values })),
    updates: Object.values(state.edits).map((e) => ({ key: e.key, values: e.values, original: e.original })),
    deletes: Object.values(state.deletes).map((key) => ({ key })),
  };
}

/** Maps a failing change (kind + index in the payload) back to the grid row. */
export function changeTarget(
  state: StagedChanges,
  kind: ChangeKind | null,
  index: number | null,
): { rowId?: string; insertId?: string } {
  if (kind === null || index === null) return {};
  if (kind === "insert") return { insertId: state.inserts[index]?.id };
  const ids = Object.keys(kind === "update" ? state.edits : state.deletes);
  return { rowId: ids[index] };
}

// ---------------------------------------------------------------------------
// Filter builder model
// ---------------------------------------------------------------------------

export interface UiCondition {
  id: string;
  column: string;
  operator: FilterOperator;
  value: string;
  /** Upper bound for `between`. */
  value2: string;
}

export interface UiGroup {
  id: string;
  op: "and" | "or";
  items: UiNode[];
}

export type UiNode = UiCondition | UiGroup;

export const MAX_FILTER_DEPTH = 3;

export const isGroup = (node: UiNode): node is UiGroup => "items" in node;

export const OPERATORS: { value: FilterOperator; label: string; arity: 0 | 1 | 2 | "list" }[] = [
  { value: "=", label: "=", arity: 1 },
  { value: "!=", label: "≠", arity: 1 },
  { value: ">", label: ">", arity: 1 },
  { value: ">=", label: "≥", arity: 1 },
  { value: "<", label: "<", arity: 1 },
  { value: "<=", label: "≤", arity: 1 },
  { value: "like", label: "like", arity: 1 },
  { value: "ilike", label: "ilike", arity: 1 },
  { value: "in", label: "in", arity: "list" },
  { value: "between", label: "between", arity: 2 },
  { value: "is_null", label: "is null", arity: 0 },
  { value: "is_not_null", label: "is not null", arity: 0 },
];

export const arityOf = (op: FilterOperator) => OPERATORS.find((o) => o.value === op)?.arity ?? 1;

/** Operators that make sense for a column kind. */
export function operatorsFor(kind: ColumnKind | undefined): FilterOperator[] {
  switch (kind) {
    case "boolean":
      return ["=", "!=", "is_null", "is_not_null"];
    case "json":
    case "array":
    case "binary":
      return ["=", "!=", "ilike", "is_null", "is_not_null"];
    case "enum":
      return ["=", "!=", "in", "is_null", "is_not_null"];
    default:
      return OPERATORS.map((o) => o.value);
  }
}

export const newCondition = (column = ""): UiCondition => ({
  id: nextId("c"),
  column,
  operator: "=",
  value: "",
  value2: "",
});

export const newGroup = (op: "and" | "or" = "and", items: UiNode[] = []): UiGroup => ({ id: nextId("g"), op, items });

/** Applies `fn` to the node with `id` (returning null removes it). */
export function updateNode(group: UiGroup, id: string, fn: (node: UiNode) => UiNode | null): UiGroup {
  const items: UiNode[] = [];
  for (const item of group.items) {
    if (item.id === id) {
      const next = fn(item);
      if (next) items.push(next);
    } else {
      items.push(isGroup(item) ? updateNode(item, id, fn) : item);
    }
  }
  return { ...group, items };
}

export function countConditions(group: UiGroup): number {
  return group.items.reduce((n, item) => n + (isGroup(item) ? countConditions(item) : item.column ? 1 : 0), 0);
}

/** Number of conditions in an API filter (nested groups included). */
export function countFilterConditions(filter: FilterGroup | null): number {
  if (!filter) return 0;
  return filter.items.reduce((n, item) => n + ("items" in item ? countFilterConditions(item) : 1), 0);
}

/** Converts the builder model to the API filter; incomplete rows are skipped. */
export function toApiFilter(group: UiGroup): FilterGroup | null {
  const items: (FilterCondition | FilterGroup)[] = [];
  for (const item of group.items) {
    if (isGroup(item)) {
      const sub = toApiFilter(item);
      if (sub) items.push(sub);
      continue;
    }
    if (!item.column) continue;
    const arity = arityOf(item.operator);
    if (arity === 0) items.push({ column: item.column, operator: item.operator });
    else if (arity === "list") {
      const values = item.value
        .split(",")
        .map((v) => v.trim())
        .filter(Boolean);
      if (values.length > 0) items.push({ column: item.column, operator: "in", value: values });
    } else if (arity === 2) items.push({ column: item.column, operator: "between", value: [item.value, item.value2] });
    else items.push({ column: item.column, operator: item.operator, value: item.value });
  }
  return items.length > 0 ? { op: group.op, items } : null;
}

// ---------------------------------------------------------------------------
// Column visibility and copying
// ---------------------------------------------------------------------------

export const visibilityKey = (connectionId: string, schema: string, table: string) =>
  `querylab.studio.hidden.${connectionId}.${schema}.${table}`;

export function loadHidden(key: string): string[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === "string") : [];
  } catch {
    return [];
  }
}

export function saveHidden(key: string, hidden: string[]) {
  try {
    if (hidden.length === 0) localStorage.removeItem(key);
    else localStorage.setItem(key, JSON.stringify(hidden));
  } catch {
    // Storage may be unavailable (private mode); visibility is a convenience.
  }
}

/** Tab-separated rows with a header, for pasting into spreadsheets. */
export function rowsToTsv(columns: string[], rows: DataCell[][]): string {
  const clean = (v: DataCell) => (v === null ? "" : v.replace(/[\t\r\n]+/g, " "));
  return [columns.join("\t"), ...rows.map((r) => r.map(clean).join("\t"))].join("\n");
}

/** Rows as JSON objects keyed by column name. */
export function rowsToJson(columns: string[], rows: DataCell[][]): string {
  return JSON.stringify(
    rows.map((r) => Object.fromEntries(columns.map((c, i) => [c, r[i]]))),
    null,
    2,
  );
}
