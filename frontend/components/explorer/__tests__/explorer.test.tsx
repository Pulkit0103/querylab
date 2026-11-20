// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ObjectDetails } from "@/components/explorer/object-details";
import { SchemaTree } from "@/components/explorer/schema-tree";
import type { ColumnInfo, FunctionDetail, SchemaObjects, TableInfo } from "@/types/schema";

afterEach(cleanup);

const CONN = "c1";

const tables: TableInfo[] = [
  { schema: "inventory", name: "products", kind: "table", estimated_rows: 4, total_bytes: 8192, comment: null },
  { schema: "inventory", name: "stock_summary", kind: "materialized_view", estimated_rows: 3, total_bytes: null, comment: null },
];

const columns: ColumnInfo[] = [
  { name: "id", ordinal: 1, data_type: "bigint", nullable: false, default_value: null, is_primary_key: true, foreign_key: null, comment: null },
  { name: "name", ordinal: 2, data_type: "text", nullable: false, default_value: null, is_primary_key: false, foreign_key: null, comment: null },
];

function client() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  qc.setQueryData(["schema", CONN, "schemas"], [{ name: "inventory", table_count: 2 }]);
  qc.setQueryData(["schema", CONN, "tables"], tables);
  qc.setQueryData(["schema", CONN, "table", "inventory", "products", "columns"], columns);
  return qc;
}

function renderWith(ui: ReactNode, qc = client()) {
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

describe("SchemaTree", () => {
  it("separates tables and views and offers object folders", () => {
    renderWith(<SchemaTree connectionId={CONN} onSelectTable={() => {}} onSelectObject={() => {}} />);
    fireEvent.click(screen.getByText("inventory"));
    expect(screen.getByText("Tables")).toBeTruthy();
    expect(screen.getByText("Views")).toBeTruthy();
    expect(screen.getByText("products")).toBeTruthy();
    expect(screen.getByText("stock_summary")).toBeTruthy();
    for (const folder of ["Functions", "Sequences", "Types"]) expect(screen.getByText(folder)).toBeTruthy();
  });

  it("opens a right-click menu that generates SQL into a new tab", async () => {
    const onOpenSql = vi.fn();
    renderWith(<SchemaTree connectionId={CONN} onSelectTable={() => {}} onOpenSql={onOpenSql} onPreviewTable={() => {}} />);
    fireEvent.click(screen.getByText("inventory"));
    fireEvent.contextMenu(screen.getByText("products"));
    const item = await screen.findByRole("menuitem", { name: /Generate SELECT/ });
    expect(screen.getByRole("menuitem", { name: /Generate INSERT/ })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: /Generate CREATE TABLE/ })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: /Copy qualified name/ })).toBeTruthy();
    fireEvent.click(item);
    await waitFor(() => expect(onOpenSql).toHaveBeenCalledOnce());
    expect(onOpenSql).toHaveBeenCalledWith("products", "SELECT id,\n       name\nFROM inventory.products\nLIMIT 100;\n");
  });

  it("filters tables and shows column and function matches from the server", () => {
    const qc = client();
    qc.setQueryData(
      ["schema", CONN, "search", "pr"],
      [
        { kind: "column", schema: "inventory", table: "products", name: "price", detail: "numeric(10,2)" },
        { kind: "function", schema: "inventory", table: null, name: "restock", detail: "p_id bigint" },
        { kind: "table", schema: "inventory", table: null, name: "products", detail: null },
      ],
    );
    const onSelectTable = vi.fn();
    renderWith(<SchemaTree connectionId={CONN} onSelectTable={onSelectTable} onSelectObject={() => {}} />, qc);
    fireEvent.change(screen.getByLabelText("Search schema"), { target: { value: "pr" } });
    expect(screen.queryByText("stock_summary")).toBeNull();
    const group = screen.getByRole("group", { name: "Column and function matches" });
    expect(group.textContent).toContain("products.price");
    expect(group.textContent).toContain("restock(p_id bigint)");
    fireEvent.click(screen.getByText("products.price"));
    expect(onSelectTable).toHaveBeenCalledWith("inventory", "products");
  });
});

describe("ObjectDetails", () => {
  it("shows a function's definition with editor action", () => {
    const qc = client();
    const detail: FunctionDetail = {
      oid: 42,
      schema: "inventory",
      name: "stock_value",
      kind: "function",
      arguments: "p_status inventory.stock_status",
      result: "numeric",
      language: "sql",
      volatility: "stable",
      security_definer: false,
      comment: "Value of stock on hand",
      definition: "CREATE OR REPLACE FUNCTION inventory.stock_value(...)",
    };
    qc.setQueryData(["schema", CONN, "function", 42], detail);
    const onOpenSql = vi.fn();
    renderWith(
      <ObjectDetails connectionId={CONN} object={{ kind: "function", oid: 42, schema: "inventory", name: "stock_value" }} onOpenSql={onOpenSql} />,
      qc,
    );
    expect(screen.getByText("Value of stock on hand")).toBeTruthy();
    expect(screen.getByText("stable")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Open in editor/ }));
    expect(onOpenSql).toHaveBeenCalledWith("stock_value.sql", detail.definition);
  });

  it("lists enum values", () => {
    const qc = client();
    const objects: SchemaObjects = {
      sequences: [],
      types: [{ schema: "inventory", name: "stock_status", kind: "enum", detail: null, values: ["in_stock", "low"], comment: null }],
    };
    qc.setQueryData(["schema", CONN, "objects"], objects);
    renderWith(<ObjectDetails connectionId={CONN} object={{ kind: "type", schema: "inventory", name: "stock_status" }} />, qc);
    expect(screen.getByText("in_stock")).toBeTruthy();
    expect(screen.getByText("low")).toBeTruthy();
  });
});
