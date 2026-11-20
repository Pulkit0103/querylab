/**
 * Pure helpers for ER diagrams: node geometry, crow's foot notation, edge
 * routing sides, automatic layout (ELK layered) and saved positions.
 */
import type { ElkNode } from "elkjs/lib/elk-api";

import type { GraphEdge, GraphTable, Positions } from "@/types/diagram";

// ---------------------------------------------------------------------------
// Geometry (the node component renders exactly these sizes)
// ---------------------------------------------------------------------------

export const HEADER_HEIGHT = 40;
export const ROW_HEIGHT = 22;
export const NODE_PADDING = 6;
/** Columns listed per node; the rest are summarised as "+N more". */
export const MAX_NODE_COLUMNS = 30;
export const MIN_NODE_WIDTH = 200;
export const MAX_NODE_WIDTH = 360;
/** Schemas larger than this start with a subset of tables. */
export const LARGE_SCHEMA_TABLES = 150;
/** Saved positions per diagram (mirrors the server). */
export const MAX_LAYOUT_NODES = 2000;

const MONO_CHAR = 6.6;
const HEADER_CHAR = 7.4;

export interface NodeSize {
  width: number;
  height: number;
  /** Columns listed in the node. */
  visible: number;
  /** Columns summarised as "+N more". */
  hidden: number;
}

export function nodeSize(table: GraphTable): NodeSize {
  const visible = Math.min(table.columns.length, MAX_NODE_COLUMNS);
  const hidden = table.columns.length - visible;
  // An empty table still shows one "no columns" row.
  const rows = Math.max(visible, 1) + (hidden > 0 ? 1 : 0);
  const widest = Math.max(
    table.name.length * HEADER_CHAR + 64,
    ...table.columns.slice(0, visible).map((c) => (c.name.length + c.data_type.length) * MONO_CHAR + 64),
  );
  return {
    width: Math.round(Math.min(MAX_NODE_WIDTH, Math.max(MIN_NODE_WIDTH, widest))),
    height: HEADER_HEIGHT + rows * ROW_HEIGHT + NODE_PADDING,
    visible,
    hidden,
  };
}

/**
 * Vertical offset (from the node top) where an edge attaches for a column:
 * the column's row, or the "+N more" row for columns that are not listed.
 */
export function rowAnchor(table: GraphTable, column: string | undefined): number {
  const { visible, hidden } = nodeSize(table);
  const index = column === undefined ? -1 : table.columns.findIndex((c) => c.name === column);
  if (index < 0) return HEADER_HEIGHT / 2;
  const row = index < visible ? index : hidden > 0 ? visible : visible - 1;
  return HEADER_HEIGHT + row * ROW_HEIGHT + ROW_HEIGHT / 2;
}

// ---------------------------------------------------------------------------
// Crow's foot notation
// ---------------------------------------------------------------------------

export type EndKind = "one" | "zero_or_one" | "zero_or_many";

/**
 * Notation at each end of a foreign key. The parent end shows how many
 * parents a child row has; the child end how many children a parent row has.
 */
export function edgeEnds(edge: GraphEdge): { child: EndKind; parent: EndKind } {
  return {
    child: edge.cardinality === "one_to_one" ? "zero_or_one" : "zero_or_many",
    parent: edge.optional ? "zero_or_one" : "one",
  };
}

export type Glyph =
  | { type: "line"; x1: number; y1: number; x2: number; y2: number }
  | { type: "circle"; cx: number; cy: number; r: number };

/**
 * Marker shapes for an edge end at (x, y). `dir` is +1 when the edge leaves
 * the node to the right and -1 when it leaves to the left.
 */
export function endGlyph(kind: EndKind, x: number, y: number, dir: 1 | -1): Glyph[] {
  const bar = (d: number): Glyph => ({ type: "line", x1: x + dir * d, y1: y - 6, x2: x + dir * d, y2: y + 6 });
  const ring: Glyph = { type: "circle", cx: x + dir * 18, cy: y, r: 4 };
  switch (kind) {
    case "one":
      return [bar(8), bar(13)];
    case "zero_or_one":
      return [bar(8), ring];
    case "zero_or_many":
      return [
        { type: "line", x1: x + dir * 12, y1: y, x2: x, y2: y - 7 },
        { type: "line", x1: x + dir * 12, y1: y, x2: x, y2: y + 7 },
        ring,
      ];
  }
}

const ACTION_LABEL: Record<GraphEdge["on_delete"], string> = {
  no_action: "no action",
  restrict: "restrict",
  cascade: "cascade",
  set_null: "set null",
  set_default: "set default",
};

/** Plain-language summary of a relationship (tooltips and screen readers). */
export function describeEdge(edge: GraphEdge): string {
  const kind = edge.cardinality === "one_to_one" ? "one-to-one" : "one-to-many";
  const from = `${edge.from_table} (${edge.from_columns.join(", ")})`;
  const to = `${edge.to_table} (${edge.to_columns.join(", ")})`;
  const parts = [`${from} → ${to}`, `${kind}, ${edge.optional ? "optional" : "required"}`];
  if (edge.on_delete !== "no_action") parts.push(`on delete ${ACTION_LABEL[edge.on_delete]}`);
  if (edge.on_update !== "no_action") parts.push(`on update ${ACTION_LABEL[edge.on_update]}`);
  return `${parts.join(" · ")} · ${edge.constraint}`;
}

// ---------------------------------------------------------------------------
// Edge sides
// ---------------------------------------------------------------------------

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type Side = "left" | "right";

const SIDE_GAP = 24;

/** Which side of each node an edge leaves from, so lines do not cross nodes. */
export function edgeSides(source: Box, target: Box, selfLoop: boolean): { source: Side; target: Side } {
  if (selfLoop) return { source: "right", target: "right" };
  if (source.x + source.width + SIDE_GAP <= target.x) return { source: "right", target: "left" };
  if (target.x + target.width + SIDE_GAP <= source.x) return { source: "left", target: "right" };
  // Stacked vertically: route around one side.
  return source.x <= target.x ? { source: "left", target: "left" } : { source: "right", target: "right" };
}

/** Rounded loop on the right of a node for self-referencing keys. */
export function selfLoopPath(x: number, y1: number, y2: number, reach = 36): string {
  const top = Math.min(y1, y2);
  const bottom = Math.max(y1, y2) === top ? top + 12 : Math.max(y1, y2);
  const r = Math.min(8, (bottom - top) / 2);
  return [
    `M ${x} ${y1}`,
    `H ${x + reach - r}`,
    `Q ${x + reach} ${y1} ${x + reach} ${y1 + (y2 >= y1 ? r : -r)}`,
    `V ${y2 - (y2 >= y1 ? r : -r)}`,
    `Q ${x + reach} ${y2} ${x + reach - r} ${y2}`,
    `H ${x}`,
  ].join(" ");
}

// ---------------------------------------------------------------------------
// Search and highlighting
// ---------------------------------------------------------------------------

/** Tables matching a search, best first: exact, prefix, then substring. */
export function findTables(tables: GraphTable[], query: string): string[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const rank = (name: string) => (name === q ? 0 : name.startsWith(q) ? 1 : 2);
  return tables
    .map((t) => t.name)
    .filter((name) => name.toLowerCase().includes(q))
    .sort((a, b) => rank(a.toLowerCase()) - rank(b.toLowerCase()) || a.localeCompare(b));
}

/** A table and every table it shares a foreign key with. */
export function neighbourhood(edges: GraphEdge[], table: string): Set<string> {
  const out = new Set([table]);
  for (const e of edges) {
    if (e.from_table === table) out.add(e.to_table);
    if (e.to_table === table) out.add(e.from_table);
  }
  return out;
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

const LAYOUT_OPTIONS: Record<string, string> = {
  "elk.algorithm": "layered",
  // Referenced (parent) tables on the left, children to the right.
  "elk.direction": "RIGHT",
  "elk.layered.spacing.nodeNodeBetweenLayers": "96",
  "elk.spacing.nodeNode": "36",
  "elk.layered.nodePlacement.strategy": "BRANDES_KOEPF",
  "elk.layered.crossingMinimization.strategy": "LAYER_SWEEP",
  "elk.separateConnectedComponents": "true",
  "elk.spacing.componentComponent": "72",
  "elk.aspectRatio": "1.6",
};

/** ELK input for the given tables and the edges between them. */
export function elkGraph(tables: GraphTable[], edges: GraphEdge[]): ElkNode {
  const names = new Set(tables.map((t) => t.name));
  return {
    id: "root",
    layoutOptions: LAYOUT_OPTIONS,
    children: tables.map((t) => {
      const { width, height } = nodeSize(t);
      return { id: t.name, width, height };
    }),
    edges: edges
      .filter((e) => e.from_table !== e.to_table && names.has(e.from_table) && names.has(e.to_table))
      .map((e) => ({ id: `${e.from_table}:${e.constraint}`, sources: [e.to_table], targets: [e.from_table] })),
  };
}

type ElkLayout = { layout: (graph: ElkNode) => Promise<ElkNode> };
let elkInstance: Promise<ElkLayout> | null = null;

/** Loads ELK on first use so it is not part of every page's bundle. */
function elk(): Promise<ElkLayout> {
  elkInstance ??= import("elkjs/lib/elk.bundled.js").then((m) => new m.default());
  return elkInstance;
}

export async function autoLayout(tables: GraphTable[], edges: GraphEdge[]): Promise<Positions> {
  if (tables.length === 0) return {};
  const result = await (await elk()).layout(elkGraph(tables, edges));
  const out: Positions = {};
  for (const child of result.children ?? []) {
    out[child.id] = { x: Math.round(child.x ?? 0), y: Math.round(child.y ?? 0) };
  }
  return out;
}

/**
 * Positions for every table in the graph: saved ones are kept, tables
 * without a saved position are laid out together and placed below them.
 */
export async function resolvePositions(
  tables: GraphTable[],
  edges: GraphEdge[],
  saved: Positions,
): Promise<Positions> {
  const missing = tables.filter((t) => !saved[t.name]);
  const kept: Positions = {};
  for (const t of tables) if (saved[t.name]) kept[t.name] = saved[t.name];
  if (missing.length === 0) return kept;
  const placed = await autoLayout(missing, edges);
  if (missing.length === tables.length) return placed;

  const keptTables = tables.filter((t) => kept[t.name]);
  const minX = Math.min(...keptTables.map((t) => kept[t.name].x));
  const maxY = Math.max(...keptTables.map((t) => kept[t.name].y + nodeSize(t).height));
  const placedMinX = Math.min(...Object.values(placed).map((p) => p.x));
  const placedMinY = Math.min(...Object.values(placed).map((p) => p.y));
  for (const [name, p] of Object.entries(placed)) {
    kept[name] = { x: p.x - placedMinX + minX, y: p.y - placedMinY + maxY + 96 };
  }
  return kept;
}

/**
 * Positions to save: the current ones over previously saved ones (tables
 * outside the current selection keep their place), within the server limit.
 */
export function mergePositions(saved: Positions, current: Positions): Positions {
  const round = (n: number) => Math.round(n * 10) / 10;
  const out: Positions = {};
  for (const [name, p] of Object.entries(current)) out[name] = { x: round(p.x), y: round(p.y) };
  for (const [name, p] of Object.entries(saved)) {
    if (Object.keys(out).length >= MAX_LAYOUT_NODES) break;
    if (!out[name]) out[name] = p;
  }
  return out;
}
