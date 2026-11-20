// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useState, type ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ChangePanel } from "@/components/data-studio/change-panel";
import { DataGrid } from "@/components/data-studio/data-grid";
import { FilterBuilder } from "@/components/data-studio/filter-builder";
import {
  addInsert,
  emptyChanges,
  newCondition,
  newGroup,
  rowRef,
  setInsertValue,
  stageEdit,
  type StagedChanges,
  type UiGroup,
} from "@/lib/data-studio";
import type { DataColumn } from "@/types/data";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const column = (name: string, extra: Partial<DataColumn> = {}): DataColumn => ({
  name,
  data_type: "text",
  type_name: "text",
  kind: "text",
  nullable: true,
  has_default: false,
  is_key: false,
  identity: null,
  generated: false,
  editable: true,
  enum_values: [],
  ...extra,
});

const columns = [
  column("id", { is_key: true, kind: "number", data_type: "integer", editable: false, identity: "always", nullable: false }),
  column("name", { nullable: false }),
  column("age", { kind: "number", data_type: "integer" }),
];

function withClient(ui: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return <QueryClientProvider client={qc}>{ui}</QueryClientProvider>;
}

/** Replies to API calls by path suffix and records request bodies. */
function mockApi(routes: Record<string, unknown>) {
  const calls: { path: string; body: unknown }[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const path = String(url);
      calls.push({ path, body: init?.body ? JSON.parse(String(init.body)) : undefined });
      const key = Object.keys(routes).find((suffix) => path.endsWith(suffix));
      return new Response(JSON.stringify(key ? routes[key] : {}), {
        status: key ? 200 : 404,
        headers: { "content-type": "application/json" },
      });
    }),
  );
  return calls;
}

describe("FilterBuilder", () => {
  function Harness({ onApply, initial }: { onApply: () => void; initial: UiGroup }) {
    const [value, setValue] = useState(initial);
    return (
      <FilterBuilder
        columns={columns}
        value={value}
        onChange={setValue}
        onApply={onApply}
        onClear={() => setValue(newGroup())}
        sql="SELECT * FROM public.people WHERE age > 25 ORDER BY id LIMIT 100"
        dirty
      />
    );
  }

  it("edits conditions, nests groups and shows the server SQL", () => {
    const onApply = vi.fn();
    render(<Harness onApply={onApply} initial={newGroup("and", [{ ...newCondition("age"), operator: ">" }])} />);

    const value = screen.getByLabelText("Value");
    fireEvent.change(value, { target: { value: "25" } });
    expect((screen.getByLabelText("Value") as HTMLInputElement).value).toBe("25");
    fireEvent.keyDown(screen.getByLabelText("Value"), { key: "Enter" });
    expect(onApply).toHaveBeenCalledOnce();

    expect(screen.getByLabelText("Generated SQL").textContent).toContain("WHERE age > 25");

    // A nested group with its own condition, and the AND/OR switch appears.
    fireEvent.click(screen.getByRole("button", { name: /Group/ }));
    const nested = screen.getByRole("group", { name: "Condition group" });
    expect(within(nested).getAllByTestId("filter-condition")).toHaveLength(1);
    const any = screen.getAllByRole("button", { name: "any (OR)" })[0];
    fireEvent.click(any);
    expect(any.getAttribute("aria-pressed")).toBe("true");

    // Removing the nested group leaves the first condition.
    fireEvent.click(screen.getByRole("button", { name: "Remove group" }));
    expect(screen.getAllByTestId("filter-condition")).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Apply filter" }));
    expect(onApply).toHaveBeenCalledTimes(2);
  });

  it("shows a two-value editor for between", () => {
    render(<Harness onApply={() => {}} initial={newGroup("and", [{ ...newCondition("age"), operator: "between" }])} />);
    expect(screen.getByLabelText("Value")).toBeTruthy();
    expect(screen.getByLabelText("Upper bound")).toBeTruthy();
  });
});

describe("DataGrid", () => {
  const rows = [
    ["101", "Ada", "36"],
    ["102", "Grace", null],
  ];

  function Harness({ initial = emptyChanges(), canUpdate = true }: { initial?: StagedChanges; canUpdate?: boolean }) {
    const [changes, setChanges] = useState(initial);
    return (
      <DataGrid
        columns={columns.map((c, index) => ({ column: c, index }))}
        allColumns={columns}
        rows={rows}
        keyColumns={["id"]}
        rowOffset={0}
        canUpdate={canUpdate}
        canDelete
        changes={changes}
        onChanges={(fn) => setChanges(fn)}
        sort={[]}
        onSort={() => {}}
        selected={new Set()}
        onSelectedChange={() => {}}
      />
    );
  }

  it("stages a cell edit, highlights it and marks deleted rows", () => {
    render(<Harness />);
    fireEvent.doubleClick(screen.getByText("Ada"));
    const editor = screen.getByRole("dialog", { name: "Edit name" });
    fireEvent.change(within(editor).getByLabelText("Value"), { target: { value: "Ada L." } });
    fireEvent.keyDown(within(editor).getByLabelText("Value"), { key: "Enter" });
    const cell = screen.getByText("Ada L.").closest("td")!;
    expect(cell.getAttribute("data-staged")).toBe("true");

    fireEvent.click(screen.getByRole("button", { name: "Delete row 2" }));
    expect(screen.getByText("Grace").closest("tr")!.getAttribute("data-row-state")).toBe("deleted");
    fireEvent.click(screen.getByRole("button", { name: "Keep row 2" }));
    expect(screen.getByText("Grace").closest("tr")!.getAttribute("data-row-state")).toBeNull();
  });

  it("never opens editors for read-only columns or connections", () => {
    const { unmount } = render(<Harness />);
    fireEvent.doubleClick(screen.getByText("101"));
    expect(screen.queryByRole("dialog")).toBeNull();
    unmount();
    render(<Harness canUpdate={false} />);
    fireEvent.doubleClick(screen.getByText("Ada"));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows new rows with defaults and NULL toggles", () => {
    let initial = addInsert(emptyChanges(), "n1");
    initial = setInsertValue(initial, "n1", "name", "Linus");
    render(<Harness initial={initial} />);
    const row = screen.getByText("Linus").closest("tr")!;
    expect(row.getAttribute("data-row-state")).toBe("inserted");
    expect(within(row).getByText("DEFAULT")).toBeTruthy(); // identity id
    fireEvent.doubleClick(within(row).getByText("NULL")); // age
    fireEvent.click(within(screen.getByRole("dialog", { name: "Edit age" })).getByRole("button", { name: "Default" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("ChangePanel", () => {
  const ref = rowRef(columns, ["id"], ["1", "Ada", "36"])!;
  const staged = () => setInsertValue(addInsert(stageEdit(emptyChanges(), ref, "age", "37", "36"), "n1"), "n1", "name", "Linus");

  it("renders the server SQL and applies with original values", async () => {
    const calls = mockApi({
      "/data/preview": {
        select_sql: "SELECT * FROM public.people",
        statements: [
          { kind: "update", index: 0, sql: "UPDATE public.people SET age = 37 WHERE id = 1;" },
          { kind: "insert", index: 0, sql: "INSERT INTO public.people (name) VALUES ('Linus');" },
        ],
      },
      "/data/apply": { committed: true, applied: [], failure: null, rows_affected: 2, execution_time_ms: 3, history_id: null },
    });
    const onApplied = vi.fn();
    render(
      withClient(
        <ChangePanel
          connection={{ id: "c1", name: "Dev", environment: "development" }}
          schema="public"
          table="people"
          changes={staged()}
          onDiscard={() => {}}
          onApplied={onApplied}
          onFailure={() => {}}
        />,
      ),
    );
    expect(screen.getByText("2 pending changes")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Review SQL/ }));
    expect(await screen.findByText("UPDATE public.people SET age = 37 WHERE id = 1;")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /Apply changes/ }));
    await waitFor(() => expect(onApplied).toHaveBeenCalledOnce());
    const apply = calls.find((c) => c.path.endsWith("/data/apply"))!;
    expect(apply.body).toEqual({
      schema: "public",
      table: "people",
      changes: {
        inserts: [{ values: { name: "Linus" } }],
        updates: [{ key: { id: "1" }, values: { age: "37" }, original: { age: "36" } }],
        deletes: [],
      },
    });
  });

  it("reports a rolled-back apply and points at the failing row", async () => {
    mockApi({
      "/data/apply": {
        committed: false,
        applied: [],
        failure: {
          kind: "update",
          index: 0,
          conflict: true,
          message: "The row was changed or deleted after it was loaded.",
          sql_error: null,
          statement: "UPDATE …",
        },
        rows_affected: 0,
        execution_time_ms: 2,
        history_id: null,
      },
    });
    const onFailure = vi.fn();
    render(
      withClient(
        <ChangePanel
          connection={{ id: "c1", name: "Dev", environment: "development" }}
          schema="public"
          table="people"
          changes={staged()}
          onDiscard={() => {}}
          onApplied={() => {}}
          onFailure={onFailure}
        />,
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: /Apply changes/ }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Nothing was applied");
    expect(alert.textContent).toContain("changed or deleted");
    expect(onFailure).toHaveBeenCalledWith({ rowId: ref.id });
  });

  it("requires the connection name on production", async () => {
    const calls = mockApi({
      "/data/apply": { committed: true, applied: [], failure: null, rows_affected: 2, execution_time_ms: 3, history_id: null },
    });
    render(
      withClient(
        <ChangePanel
          connection={{ id: "c1", name: "Billing", environment: "production" }}
          schema="public"
          table="people"
          changes={staged()}
          onDiscard={() => {}}
          onApplied={() => {}}
          onFailure={() => {}}
        />,
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: /Apply changes/ }));
    const dialog = await screen.findByRole("alertdialog");
    const confirm = within(dialog).getByRole("button", { name: "Apply changes" }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
    fireEvent.change(within(dialog).getByLabelText("Connection name"), { target: { value: "Billing" } });
    expect(confirm.disabled).toBe(false);
    fireEvent.click(confirm);
    await waitFor(() => expect(calls.some((c) => c.path.endsWith("/data/apply"))).toBe(true));
    expect((calls.find((c) => c.path.endsWith("/data/apply"))!.body as { confirm: string }).confirm).toBe("Billing");
  });
});
