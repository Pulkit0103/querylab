// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { quickOpenItems } from "@/components/layout/quick-open";
import { PinButton } from "@/components/library/pin-button";
import { TagEditor } from "@/components/library/tag-editor";
import type { HistoryEntry, SavedQuery } from "@/types/library";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("TagEditor", () => {
  function Harness({ initial = [] as string[] }) {
    const [tags, setTags] = useState(initial);
    return (
      <>
        <TagEditor value={tags} onChange={setTags} suggestions={["reports", "ops"]} />
        <output data-testid="tags">{tags.join("|")}</output>
      </>
    );
  }
  const tags = () => screen.getByTestId("tags").textContent;
  const input = () => screen.getByLabelText("Add tag");

  it("adds normalized tags with Enter or comma and removes them", () => {
    render(<Harness />);
    fireEvent.change(input(), { target: { value: " Reports " } });
    fireEvent.keyDown(input(), { key: "Enter" });
    fireEvent.change(input(), { target: { value: "KPI" } });
    fireEvent.keyDown(input(), { key: "," });
    expect(tags()).toBe("reports|kpi");

    fireEvent.click(screen.getByRole("button", { name: "Remove tag reports" }));
    expect(tags()).toBe("kpi");
    fireEvent.keyDown(input(), { key: "Backspace" });
    expect(tags()).toBe("");
  });

  it("rejects duplicates and over-long tags with a message", () => {
    render(<Harness initial={["ops"]} />);
    fireEvent.change(input(), { target: { value: "OPS" } });
    fireEvent.keyDown(input(), { key: "Enter" });
    expect(screen.getByRole("alert").textContent).toBe("Already added");
    fireEvent.change(input(), { target: { value: "x".repeat(41) } });
    expect(screen.queryByRole("alert")).toBeNull();
    fireEvent.keyDown(input(), { key: "Enter" });
    expect(screen.getByRole("alert").textContent).toMatch(/40/);
    expect(tags()).toBe("ops");
  });

  it("adds a pending tag on blur", () => {
    render(<Harness />);
    fireEvent.change(input(), { target: { value: "daily" } });
    fireEvent.blur(input());
    expect(tags()).toBe("daily");
  });
});

describe("PinButton", () => {
  it("toggles the pin through the API", async () => {
    const calls: { url: string; method?: string; body: unknown }[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, init?: RequestInit) => {
        calls.push({ url, method: init?.method, body: JSON.parse(String(init?.body)) });
        return new Response(JSON.stringify({ id: "q1", pinned: true }), { headers: { "content-type": "application/json" } });
      }),
    );
    const qc = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
    render(
      <QueryClientProvider client={qc}>
        <PinButton target="saved" id="q1" pinned={false} label="Revenue" />
        <PinButton target="history" id="h1" pinned label="history entry" />
      </QueryClientProvider>,
    );
    const pin = screen.getByRole("button", { name: "Pin Revenue" });
    expect(pin.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(pin);
    fireEvent.click(screen.getByRole("button", { name: "Unpin history entry" }));
    await waitFor(() => expect(calls).toHaveLength(2));
    expect(calls).toContainEqual({ url: "/api/queries/q1/pin", method: "PUT", body: { pinned: true } });
    expect(calls).toContainEqual({ url: "/api/history/h1/pin", method: "PUT", body: { pinned: false } });
  });
});

describe("quick open", () => {
  const saved = (id: string, name: string, pinned: boolean, tags: string[] = []): SavedQuery => ({
    id,
    workspace_id: "w",
    created_by: null,
    name,
    description: null,
    sql_content: `select ${id}`,
    connection_id: null,
    collection_id: null,
    created_at: "",
    updated_at: "",
    pinned,
    tags,
  });
  const entry = (id: string, sql: string): HistoryEntry => ({
    id,
    connection_id: null,
    connection_name: null,
    sql_content: sql,
    execution_time_ms: 1,
    status: "success",
    error_message: null,
    row_count: null,
    executed_at: "",
    pinned: true,
    kind: "query",
  });

  it("lists pinned items first and other saved queries only when searching", () => {
    const list = [saved("a", "Revenue", true, ["finance"]), saved("b", "Churn", false)];
    const history = [entry("h", "select * from orders")];
    expect(quickOpenItems(list, history, "").map((i) => i.key)).toEqual(["s-a", "h-h"]);
    expect(quickOpenItems(list, history, "finance").map((i) => i.key)).toEqual(["s-a"]);
    expect(quickOpenItems(list, history, "orders").map((i) => i.key)).toEqual(["h-h"]);
    expect(quickOpenItems(list, history, "churn").map((i) => i.key)).toEqual(["s-b"]);
  });
});
