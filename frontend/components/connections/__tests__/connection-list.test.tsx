// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { ConnectionList } from "@/components/connections/connection-list";
import { EnvironmentBanner } from "@/components/layout/environment-banner";
import { conn } from "@/lib/__tests__/fixtures";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { Connection, ConnectionGroup } from "@/types/api";

afterEach(cleanup);

const groups: ConnectionGroup[] = [
  { id: "g1", name: "Analytics", position: 0, connection_count: 1, created_at: "", updated_at: "" },
  { id: "g2", name: "Empty team", position: 1, connection_count: 0, created_at: "", updated_at: "" },
];

const connections: Connection[] = [
  conn({ id: "a", name: "Warehouse", group_id: "g1", environment: "production", last_status: "online", last_latency_ms: 42, last_status_at: new Date().toISOString() }),
  conn({ id: "b", name: "Local dev", host: "localhost", environment: "development" }),
  conn({ id: "c", name: "Staging API", environment: "staging", last_status: "auth_failed", last_error: "password authentication failed" }),
];

function withData(ui: ReactNode, data: Connection[] = connections) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  qc.setQueryData(["connections"], data);
  // Groups are per workspace; no workspace is selected in these tests.
  qc.setQueryData(["connection-groups", null], groups);
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

describe("ConnectionList", () => {
  it("renders groups in order, keeps empty groups as drop targets and lists ungrouped last", () => {
    withData(<ConnectionList />);
    const sections = screen.getAllByRole("region");
    expect(sections.map((s) => s.getAttribute("aria-label"))).toEqual([
      "Group Analytics",
      "Group Empty team",
      "Ungrouped connections",
    ]);
    expect(within(sections[0]).getByText("Warehouse")).toBeTruthy();
    expect(within(sections[1]).getByText("Drag connections here.")).toBeTruthy();
    expect(within(sections[2]).getByText("Local dev")).toBeTruthy();
  });

  it("shows environment badges and server-side status", () => {
    withData(<ConnectionList />);
    expect(screen.getByText("Production")).toBeTruthy();
    expect(screen.getByText("Staging")).toBeTruthy();
    expect(screen.getByRole("img", { name: "Online" })).toBeTruthy();
    expect(screen.getByRole("img", { name: "Authentication failed" })).toBeTruthy();
    expect(screen.getAllByRole("img", { name: "Not tested" })).toHaveLength(1);
    // A failed connection offers Reconnect; others Test.
    expect(screen.getByRole("button", { name: "Reconnect" })).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Test" })).toHaveLength(2);
  });

  it("filters by search text and hides sections without matches", () => {
    withData(<ConnectionList />);
    fireEvent.change(screen.getByLabelText("Search connections"), { target: { value: "warehouse" } });
    expect(screen.getAllByRole("region").map((s) => s.getAttribute("aria-label"))).toEqual(["Group Analytics"]);
    fireEvent.change(screen.getByLabelText("Search connections"), { target: { value: "nothing-matches" } });
    expect(screen.getByText("No connections match the current filters.")).toBeTruthy();
  });

  it("collapses a group", () => {
    withData(<ConnectionList />);
    const toggle = screen.getAllByRole("button", { name: /Analytics/ }).find((b) => b.hasAttribute("aria-expanded"))!;
    expect(screen.getByText("Warehouse")).toBeTruthy();
    fireEvent.click(toggle);
    expect(screen.queryByText("Warehouse")).toBeNull();
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
  });
});

describe("EnvironmentBanner", () => {
  beforeEach(() => useWorkspaceStore.setState({ activeConnectionId: null }));

  it("warns while a production connection is active", () => {
    useWorkspaceStore.setState({ activeConnectionId: "a" });
    withData(<EnvironmentBanner />);
    const banner = screen.getByRole("status");
    expect(banner.textContent).toContain("Production");
    expect(banner.textContent).toContain("Warehouse");
  });

  it("is hidden for development connections", () => {
    useWorkspaceStore.setState({ activeConnectionId: "b" });
    withData(<EnvironmentBanner />);
    expect(screen.queryByRole("status")).toBeNull();
  });
});
