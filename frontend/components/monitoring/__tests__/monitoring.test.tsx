// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { LocksPanel } from "@/components/monitoring/locks-panel";
import type { LocksResponse, Session } from "@/types/monitoring";

vi.mock("next/navigation", () => ({ useRouter: () => ({ push: vi.fn(), replace: vi.fn() }) }));

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const session = (pid: number, s: Partial<Session> = {}): Session => ({
  pid,
  user: "owner",
  application: "app",
  client: null,
  backend_type: "client backend",
  state: "active",
  wait_event_type: null,
  wait_event: null,
  backend_start: null,
  xact_start: null,
  query_start: null,
  query_ms: 1200,
  xact_ms: 90_000,
  query: null,
  query_hidden: false,
  blocked_by: [],
  is_self: false,
  ...s,
});

const locks: LocksResponse = {
  privileges: { role: "reader", superuser: false, read_all_stats: false, signal_backend: false, read_only_connection: true },
  tree: [{ pid: 10, blocked_by: [], children: [{ pid: 11, blocked_by: [10], children: [] }] }],
  sessions: {
    "10": session(10, { state: "idle in transaction", query: "UPDATE t SET v = 1" }),
    "11": session(11, { blocked_by: [10], wait_event_type: "Lock", query_hidden: true }),
  },
  locks: [{ pid: 11, locktype: "transactionid", mode: "ShareLock", granted: false, relation: null }],
  waiting: 1,
};

function renderPanel(body: unknown) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <LocksPanel connection={{ id: "c1", name: "Prod", environment: "production" }} interval={0} onIntervalChange={() => {}} />
    </QueryClientProvider>,
  );
}

describe("LocksPanel", () => {
  it("shows who blocks whom, with hidden statements explained", async () => {
    const { container } = renderPanel(locks);
    expect(await screen.findByText("1 session waiting")).toBeTruthy();
    const tree = screen.getByRole("region", { name: "Blocking tree" });
    const rows = tree.querySelectorAll("li");
    expect([...rows].map((r) => `${r.dataset.depth}:${r.dataset.pid}`)).toEqual(["0:10", "1:11"]);
    expect(within(rows[0]).getByText("UPDATE t SET v = 1")).toBeTruthy();
    expect(within(rows[0]).getByText("blocks 1 session")).toBeTruthy();
    expect(within(rows[1]).getByText(/hidden \(insufficient privilege\)/)).toBeTruthy();
    expect(within(rows[1]).getByText("waiting")).toBeTruthy();
    // Read-only connection: actions are disabled with the reason.
    const terminate = within(rows[0]).getByRole("button", { name: "Terminate" }) as HTMLButtonElement;
    expect(terminate.disabled).toBe(true);
    expect(terminate.title).toMatch(/read-write/);
    expect(screen.getByRole("note").textContent).toMatch(/pg_monitor/);
    expect(container.querySelector("table")?.textContent).toContain("ShareLock");
  });

  it("says so when nothing is blocked", async () => {
    renderPanel({ ...locks, tree: [], sessions: {}, locks: [], waiting: 0 });
    expect(await screen.findByText(/No blocking/)).toBeTruthy();
    expect(screen.getByText("No session is waiting for a lock")).toBeTruthy();
  });
});
