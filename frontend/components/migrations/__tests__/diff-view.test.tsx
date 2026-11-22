// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { DiffView } from "@/components/migrations/diff-view";
import type { DiffResponse } from "@/types/migrations";

vi.mock("next/navigation", () => ({ useRouter: () => ({ push: vi.fn(), replace: vi.fn() }) }));

beforeAll(() => {
  // Checkbox clicks dispatch pointer events, which jsdom lacks.
  if (!("PointerEvent" in window)) vi.stubGlobal("PointerEvent", MouseEvent);
});

afterEach(cleanup);

const diff: DiffResponse = {
  source_schema: "dev",
  target_schema: "stage",
  preamble: "SET LOCAL search_path = stage;",
  hints: ["Table clients may be customers renamed."],
  changes: [
    {
      phase: "drop_column",
      object_type: "column",
      action: "drop",
      object: "orders.legacy",
      sql: "ALTER TABLE stage.orders DROP COLUMN legacy;",
      down_sql: null,
      destructive: true,
      notes: ["The column's values are lost."],
      before: "legacy text",
      after: null,
    },
    {
      phase: "create_table",
      object_type: "table",
      action: "create",
      object: "clients",
      sql: "CREATE TABLE stage.clients (\n    id integer\n);",
      down_sql: "DROP TABLE stage.clients;",
      destructive: false,
      notes: [],
      before: null,
      after: "CREATE TABLE stage.clients (id integer);",
    },
  ],
};

const script = () => screen.getByLabelText("Migration script").textContent;

describe("DiffView", () => {
  it("selects safe changes, shows details and builds the script", () => {
    render(<DiffView diff={diff} sourceLabel="Dev.dev" />);
    expect(screen.getByText(/1 to create/)).toBeTruthy();
    expect(screen.getByText(/Table clients may be customers renamed/)).toBeTruthy();
    const drop = screen.getByRole("checkbox", { name: "Include drop orders.legacy" });
    const create = screen.getByRole("checkbox", { name: "Include create clients" });
    expect(drop.getAttribute("aria-checked")).toBe("false");
    expect(create.getAttribute("aria-checked")).toBe("true");

    // First change is shown: before/after and its warning.
    expect(screen.getByText("The column's values are lost.")).toBeTruthy();
    expect(screen.getByText("legacy text")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /Script \(1\)/ }));
    expect(script()).toBe("SET LOCAL search_path = stage;\n\nCREATE TABLE stage.clients (\n    id integer\n);\n");

    fireEvent.click(drop);
    expect(screen.getByRole("button", { name: /Script \(2\)/ })).toBeTruthy();
    expect(script()).toContain("DROP COLUMN legacy;\n\nCREATE TABLE");
    expect(screen.getByText(/1 of 1 selected/)).toBeTruthy();
  });

  it("says when there is nothing to migrate", () => {
    render(<DiffView diff={{ ...diff, changes: [], hints: [] }} sourceLabel="x" />);
    expect(screen.getByText(/The schemas match/)).toBeTruthy();
  });
});
