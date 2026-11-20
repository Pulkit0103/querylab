import { describe, expect, it } from "vitest";

import {
  autoLayout,
  edgeEnds,
  edgeSides,
  elkGraph,
  endGlyph,
  findTables,
  HEADER_HEIGHT,
  MAX_LAYOUT_NODES,
  MAX_NODE_COLUMNS,
  MAX_NODE_WIDTH,
  MIN_NODE_WIDTH,
  mergePositions,
  neighbourhood,
  nodeSize,
  resolvePositions,
  ROW_HEIGHT,
  rowAnchor,
  selfLoopPath,
  describeEdge,
} from "@/lib/diagram";
import { exportFileName, exportFrame, pixelRatioFor } from "@/lib/diagram-export";
import type { GraphColumn, GraphEdge, GraphTable } from "@/types/diagram";

const col = (name: string, extra: Partial<GraphColumn> = {}): GraphColumn => ({
  name,
  data_type: "integer",
  nullable: false,
  primary_key: false,
  foreign_key: false,
  unique: false,
  ...extra,
});

const table = (name: string, columns: GraphColumn[] = [col("id", { primary_key: true })]): GraphTable => ({
  name,
  kind: "table",
  estimated_rows: null,
  comment: null,
  columns,
});

const fk = (from: string, to: string, extra: Partial<GraphEdge> = {}): GraphEdge => ({
  constraint: `${from}_${to}_fkey`,
  from_table: from,
  from_columns: [`${to}_id`],
  to_table: to,
  to_columns: ["id"],
  cardinality: "one_to_many",
  optional: false,
  on_delete: "no_action",
  on_update: "no_action",
  ...extra,
});

describe("node geometry", () => {
  it("sizes nodes from their columns", () => {
    const small = nodeSize(table("t"));
    expect(small.width).toBe(MIN_NODE_WIDTH);
    expect(small.height).toBe(HEADER_HEIGHT + ROW_HEIGHT + 6);

    const wide = nodeSize(table("t", [col("a_really_long_column_name_for_testing", { data_type: "timestamp with time zone" })]));
    expect(wide.width).toBe(MAX_NODE_WIDTH);

    const many = nodeSize(table("t", Array.from({ length: MAX_NODE_COLUMNS + 5 }, (_, i) => col(`c${i}`))));
    expect(many.visible).toBe(MAX_NODE_COLUMNS);
    expect(many.hidden).toBe(5);
    // Listed columns plus the "+5 more" row.
    expect(many.height).toBe(HEADER_HEIGHT + (MAX_NODE_COLUMNS + 1) * ROW_HEIGHT + 6);

    // Empty tables keep one row.
    expect(nodeSize(table("t", [])).height).toBe(small.height);
  });

  it("anchors edges on the column's row", () => {
    const t = table("t", Array.from({ length: MAX_NODE_COLUMNS + 2 }, (_, i) => col(`c${i}`)));
    expect(rowAnchor(t, "c0")).toBe(HEADER_HEIGHT + ROW_HEIGHT / 2);
    expect(rowAnchor(t, "c2")).toBe(HEADER_HEIGHT + 2 * ROW_HEIGHT + ROW_HEIGHT / 2);
    // Unlisted columns attach to the "+N more" row.
    expect(rowAnchor(t, `c${MAX_NODE_COLUMNS + 1}`)).toBe(HEADER_HEIGHT + MAX_NODE_COLUMNS * ROW_HEIGHT + ROW_HEIGHT / 2);
    expect(rowAnchor(t, "missing")).toBe(HEADER_HEIGHT / 2);
  });
});

describe("crow's foot notation", () => {
  it("maps cardinality and optionality to end markers", () => {
    expect(edgeEnds(fk("a", "b"))).toEqual({ child: "zero_or_many", parent: "one" });
    expect(edgeEnds(fk("a", "b", { optional: true }))).toEqual({ child: "zero_or_many", parent: "zero_or_one" });
    expect(edgeEnds(fk("a", "b", { cardinality: "one_to_one" }))).toEqual({ child: "zero_or_one", parent: "one" });
  });

  it("draws markers on the side the edge leaves from", () => {
    const one = endGlyph("one", 100, 50, 1);
    expect(one).toHaveLength(2);
    expect(one.every((g) => g.type === "line" && g.x1 > 100)).toBe(true);
    const many = endGlyph("zero_or_many", 100, 50, -1);
    // Two prongs meeting away from the node, plus the "zero" ring.
    expect(many.filter((g) => g.type === "line")).toHaveLength(2);
    expect(many.find((g) => g.type === "circle")).toMatchObject({ cx: 82, cy: 50 });
    expect(endGlyph("zero_or_one", 0, 0, 1).map((g) => g.type)).toEqual(["line", "circle"]);
  });

  it("describes relationships in words", () => {
    const text = describeEdge(fk("books", "authors", { optional: true, on_delete: "set_null", from_columns: ["translator_id"] }));
    expect(text).toBe(
      "books (translator_id) → authors (id) · one-to-many, optional · on delete set null · books_authors_fkey",
    );
  });
});

describe("edge routing", () => {
  const box = (x: number, y = 0) => ({ x, y, width: 200, height: 100 });

  it("connects facing sides and routes around stacked nodes", () => {
    expect(edgeSides(box(0), box(400), false)).toEqual({ source: "right", target: "left" });
    expect(edgeSides(box(400), box(0), false)).toEqual({ source: "left", target: "right" });
    expect(edgeSides(box(0), box(50, 300), false)).toEqual({ source: "left", target: "left" });
    expect(edgeSides(box(0), box(0), true)).toEqual({ source: "right", target: "right" });
  });

  it("loops self references outside the node", () => {
    const d = selfLoopPath(200, 60, 100);
    expect(d.startsWith("M 200 60")).toBe(true);
    expect(d.endsWith("H 200")).toBe(true);
    expect(d).toContain("236");
  });
});

describe("search and focus", () => {
  const tables = ["orders", "order_items", "customers", "work_orders"].map((n) => table(n));

  it("ranks exact, prefix, then substring matches", () => {
    expect(findTables(tables, "ORDER")).toEqual(["order_items", "orders", "work_orders"]);
    expect(findTables(tables, "orders")).toEqual(["orders", "work_orders"]);
    expect(findTables(tables, "  ")).toEqual([]);
  });

  it("collects a table's neighbours in both directions", () => {
    const edges = [fk("order_items", "orders"), fk("orders", "customers"), fk("work_orders", "work_orders")];
    expect([...neighbourhood(edges, "orders")].sort()).toEqual(["customers", "order_items", "orders"]);
    expect([...neighbourhood(edges, "work_orders")]).toEqual(["work_orders"]);
  });
});

describe("layout", () => {
  it("builds ELK input with parents before children", () => {
    const g = elkGraph([table("a"), table("b")], [fk("b", "a"), fk("b", "b"), fk("b", "elsewhere")]);
    expect(g.children?.map((c) => c.id)).toEqual(["a", "b"]);
    // Self references and edges to tables not shown are left out.
    expect(g.edges).toEqual([{ id: "b:b_a_fkey", sources: ["a"], targets: ["b"] }]);
  });

  it("lays out without overlaps, referenced tables to the left", async () => {
    const tables = [table("customers"), table("orders"), table("items"), table("loose")];
    const edges = [fk("orders", "customers"), fk("items", "orders")];
    const pos = await autoLayout(tables, edges);
    expect(Object.keys(pos).sort()).toEqual(["customers", "items", "loose", "orders"]);
    expect(pos.customers.x).toBeLessThan(pos.orders.x);
    expect(pos.orders.x).toBeLessThan(pos.items.x);
    const rects = tables.map((t) => ({ ...pos[t.name], ...nodeSize(t) }));
    for (const [i, a] of rects.entries())
      for (const b of rects.slice(i + 1)) {
        const apart = a.x + a.width <= b.x || b.x + b.width <= a.x || a.y + a.height <= b.y || b.y + b.height <= a.y;
        expect(apart).toBe(true);
      }
  });

  it("lays out a 150-table schema in under 3 seconds", async () => {
    const tables = Array.from({ length: 150 }, (_, i) =>
      table(`t${i}`, [col("id", { primary_key: true }), col("parent_id", { foreign_key: true }), col("name", { data_type: "text" })]),
    );
    const edges = tables.slice(1).flatMap((t, i) => [
      fk(t.name, `t${Math.floor(i / 3)}`),
      ...(i % 4 === 0 ? [fk(t.name, `t${(i * 7) % 150}`, { constraint: `${t.name}_extra` })] : []),
    ]);
    const started = performance.now();
    const pos = await autoLayout(tables, edges);
    expect(performance.now() - started).toBeLessThan(3000);
    expect(Object.keys(pos)).toHaveLength(150);
  }, 10_000);

  it("keeps saved positions and places new tables below them", async () => {
    const tables = [table("a"), table("b"), table("c")];
    const saved = { a: { x: 500, y: 40 }, gone: { x: 1, y: 1 } };
    const pos = await resolvePositions(tables, [fk("c", "b")], saved);
    expect(pos.a).toEqual({ x: 500, y: 40 });
    expect(pos.gone).toBeUndefined();
    const bottomOfA = 40 + nodeSize(tables[0]).height;
    expect(Math.min(pos.b.y, pos.c.y)).toBeGreaterThan(bottomOfA);
    expect(Math.min(pos.b.x, pos.c.x)).toBe(500);
    // Everything saved: nothing is laid out again.
    expect(await resolvePositions([table("a")], [], saved)).toEqual({ a: { x: 500, y: 40 } });
  });

  it("merges positions for saving", () => {
    expect(mergePositions({ a: { x: 1, y: 1 }, b: { x: 2, y: 2 } }, { a: { x: 10.04, y: 20.06 } })).toEqual({
      a: { x: 10, y: 20.1 },
      b: { x: 2, y: 2 },
    });
    const many = Object.fromEntries(Array.from({ length: MAX_LAYOUT_NODES + 10 }, (_, i) => [`s${i}`, { x: 0, y: 0 }]));
    const merged = mergePositions(many, { current: { x: 0, y: 0 } });
    expect(Object.keys(merged)).toHaveLength(MAX_LAYOUT_NODES);
    expect(merged.current).toBeDefined();
  });
});

describe("export", () => {
  it("frames the whole diagram with padding", () => {
    expect(exportFrame({ x: -100, y: 50, width: 800, height: 400 }, 20)).toEqual({
      width: 840,
      height: 440,
      transform: "translate(120px, -30px) scale(1)",
    });
  });

  it("limits the PNG size of very large diagrams", () => {
    expect(pixelRatioFor(1000, 800)).toBe(2);
    const ratio = pixelRatioFor(20_000, 10_000);
    expect(ratio).toBeLessThan(1);
    expect(20_000 * 10_000 * ratio * ratio).toBeLessThanOrEqual(40_000_001);
  });

  it("names files after the schema", () => {
    expect(exportFileName("sales", "png")).toBe("sales-er-diagram.png");
    expect(exportFileName("my schema/x", "pdf")).toBe("my_schema_x-er-diagram.pdf");
  });
});
