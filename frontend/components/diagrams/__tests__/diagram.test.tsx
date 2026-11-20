// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { ReactFlowProvider } from "@xyflow/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { DiagramCanvas } from "@/components/diagrams/diagram-canvas";
import { TableSelect } from "@/components/diagrams/table-select";
import type { SchemaGraph } from "@/types/diagram";

// The canvas measures nodes; jsdom has no layout engine.
beforeAll(() => {
  class ResizeObserverStub {
    constructor(private cb: ResizeObserverCallback) {}
    observe(target: Element) {
      this.cb([{ target } as ResizeObserverEntry], this as unknown as ResizeObserver);
    }
    unobserve() {}
    disconnect() {}
  }
  class DOMMatrixStub {
    m22 = 1;
  }
  vi.stubGlobal("ResizeObserver", ResizeObserverStub);
  vi.stubGlobal("DOMMatrixReadOnly", DOMMatrixStub);
  // Checkbox clicks dispatch pointer events, which jsdom lacks.
  if (!("PointerEvent" in window)) vi.stubGlobal("PointerEvent", MouseEvent);
  Object.defineProperties(HTMLElement.prototype, {
    offsetHeight: { configurable: true, get: () => 100 },
    offsetWidth: { configurable: true, get: () => 200 },
  });
});

afterEach(cleanup);

const graph: SchemaGraph = {
  schema: "library",
  total_tables: 3,
  truncated: false,
  hidden_edges: 0,
  tables: [
    {
      name: "authors",
      kind: "table",
      estimated_rows: 3,
      comment: null,
      columns: [
        { name: "id", data_type: "integer", nullable: false, primary_key: true, foreign_key: false, unique: true },
        { name: "name", data_type: "text", nullable: false, primary_key: false, foreign_key: false, unique: false },
      ],
    },
    {
      name: "books",
      kind: "table",
      estimated_rows: 1200,
      comment: null,
      columns: [
        { name: "id", data_type: "integer", nullable: false, primary_key: true, foreign_key: false, unique: true },
        { name: "author_id", data_type: "integer", nullable: false, primary_key: false, foreign_key: true, unique: false },
        { name: "isbn", data_type: "text", nullable: true, primary_key: false, foreign_key: false, unique: true },
      ],
    },
    { name: "audit", kind: "view", estimated_rows: null, comment: null, columns: [] },
  ],
  edges: [
    {
      constraint: "books_author_id_fkey",
      from_table: "books",
      from_columns: ["author_id"],
      to_table: "authors",
      to_columns: ["id"],
      cardinality: "one_to_many",
      optional: false,
      on_delete: "cascade",
      on_update: "no_action",
    },
  ],
};

const positions = { authors: { x: 0, y: 0 }, books: { x: 400, y: 0 }, audit: { x: 0, y: 300 } };

function renderCanvas(props: Partial<Parameters<typeof DiagramCanvas>[0]> = {}) {
  const onSelect = vi.fn();
  const utils = render(
    <div style={{ width: 1200, height: 800 }}>
      <ReactFlowProvider>
        <DiagramCanvas
          graph={graph}
          initialPositions={positions}
          focus={null}
          onSelect={onSelect}
          onPositionsChange={() => {}}
          {...props}
        />
      </ReactFlowProvider>
    </div>,
  );
  return { ...utils, onSelect };
}

const node = (container: HTMLElement, name: string) => container.querySelector<HTMLElement>(`[data-table="${name}"]`)!;

describe("DiagramCanvas", () => {
  it("renders tables with their columns and key markers", () => {
    const { container } = renderCanvas();
    const books = node(container, "books");
    expect(within(books).getByText("books")).toBeTruthy();
    expect(within(books).getByText("author_id")).toBeTruthy();
    expect(within(books).getByText("~1,200")).toBeTruthy();
    expect(within(books).getByRole("img", { name: "Primary key" })).toBeTruthy();
    expect(within(books).getByRole("img", { name: "Foreign key" })).toBeTruthy();
    expect(within(books).getByRole("img", { name: "Unique" })).toBeTruthy();
    expect(within(node(container, "audit")).getByText("No columns")).toBeTruthy();
    // Legend explains the notation.
    expect(screen.getByText("Zero or many")).toBeTruthy();
    expect(screen.getByText("Exactly one")).toBeTruthy();
  });

  it("highlights the focused table and its neighbours", () => {
    const { container } = renderCanvas({ focus: "books" });
    expect(node(container, "books").className).toContain("ring-2");
    expect(node(container, "authors").className).toContain("border-brand/60");
    expect(node(container, "audit").className).toContain("opacity-30");
  });

  it("reports clicks on tables and on the background", () => {
    const { container, onSelect } = renderCanvas();
    fireEvent.click(node(container, "authors"));
    expect(onSelect).toHaveBeenCalledWith("authors");
    fireEvent.click(container.querySelector(".react-flow__pane")!);
    expect(onSelect).toHaveBeenLastCalledWith(null);
  });

  it("draws relationships with crow's foot markers once nodes are measured", async () => {
    const { container } = renderCanvas();
    await waitFor(() => expect(container.querySelector('[data-edge="books:books_author_id_fkey"]')).not.toBeNull());
    const edge = container.querySelector('[data-edge="books:books_author_id_fkey"]')!;
    expect(edge.querySelector("title")?.textContent).toContain("books (author_id) → authors (id) · one-to-many, required");
    // Child end: two prongs and a ring; parent end: two bars.
    expect(edge.querySelectorAll("line")).toHaveLength(4);
    expect(edge.querySelectorAll("circle")).toHaveLength(1);
  });
});

describe("TableSelect", () => {
  const open = () => fireEvent.click(screen.getByRole("button", { name: /Tables/ }));

  it("applies a subset and returns to the whole schema when everything is picked", async () => {
    const onChange = vi.fn();
    render(<TableSelect available={["a", "b", "c"]} shown={["a", "b", "c"]} selection={null} onChange={onChange} />);
    open();
    await screen.findByRole("list", { name: "Tables" });
    fireEvent.click(screen.getByRole("checkbox", { name: "b" }));
    fireEvent.click(screen.getByRole("button", { name: "Show 2 tables" }));
    expect(onChange).toHaveBeenLastCalledWith(["a", "c"]);

    open();
    await screen.findByRole("list", { name: "Tables" });
    fireEvent.click(screen.getByRole("button", { name: "Select all" }));
    fireEvent.click(screen.getByRole("button", { name: "Show 3 tables" }));
    expect(onChange).toHaveBeenLastCalledWith(null);
  });

  it("filters the list", async () => {
    render(<TableSelect available={["orders", "customers"]} shown={[]} selection={[]} onChange={() => {}} />);
    open();
    fireEvent.change(await screen.findByLabelText("Filter tables"), { target: { value: "cust" } });
    const list = screen.getByRole("list", { name: "Tables" });
    expect(within(list).queryByText("orders")).toBeNull();
    expect(within(list).getByText("customers")).toBeTruthy();
  });
});
