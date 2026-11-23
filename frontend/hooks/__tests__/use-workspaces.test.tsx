// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useAcceptInvitation, useActiveWorkspace } from "@/hooks/use-workspaces";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { Workspace } from "@/types/workspaces";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  useWorkspaceStore.setState({ activeWorkspaceId: null });
});

const personal: Workspace = {
  id: "personal",
  name: "Personal",
  personal: true,
  role: "owner",
  member_count: 1,
  created_at: "",
};
const team: Workspace = { id: "team", name: "Analytics team", personal: false, role: "viewer", member_count: 3, created_at: "" };

function Probe() {
  const { workspace } = useActiveWorkspace();
  const accept = useAcceptInvitation();
  return (
    <>
      <span data-testid="active">{workspace?.name}</span>
      <button onClick={() => accept.mutate("token")}>accept</button>
    </>
  );
}

describe("joining a workspace", () => {
  it("switches to it even before the workspace list is refetched", async () => {
    // The refetch of /api/workspaces never completes during the test.
    vi.stubGlobal(
      "fetch",
      vi.fn((url: string) =>
        String(url).includes("/accept")
          ? Promise.resolve(new Response(JSON.stringify(team), { status: 200, headers: { "content-type": "application/json" } }))
          : new Promise<Response>(() => {}),
      ),
    );
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
    qc.setQueryData(["workspaces"], [personal]);
    render(
      <QueryClientProvider client={qc}>
        <Probe />
      </QueryClientProvider>,
    );
    expect(screen.getByTestId("active").textContent).toBe("Personal");

    await act(async () => screen.getByRole("button", { name: "accept" }).click());
    await waitFor(() => expect(screen.getByTestId("active").textContent).toBe("Analytics team"));
    // Still selected after effects have settled.
    await new Promise((r) => setTimeout(r, 50));
    expect(useWorkspaceStore.getState().activeWorkspaceId).toBe("team");
  });
});
