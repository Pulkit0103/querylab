// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ExplainMenu } from "@/components/editor/explain-analyze";

afterEach(cleanup);

const target = { sql: "SELECT * FROM sales.orders", offset: 4 };

async function openAnalyze() {
  fireEvent.click(screen.getByRole("button", { name: "More explain options" }));
  const item = await screen.findByRole("menuitem", { name: /Explain Analyze/ });
  fireEvent.click(item);
  return screen.findByRole("alertdialog");
}

describe("ExplainMenu", () => {
  it("requires confirmation before running EXPLAIN ANALYZE", async () => {
    const onExplain = vi.fn();
    render(<ExplainMenu disabled={false} getTarget={() => target} onExplain={onExplain} />);

    const dialog = await openAnalyze();
    expect(dialog.textContent).toContain("SELECT * FROM sales.orders");
    expect(dialog.textContent).toContain("read-only transaction");
    expect(onExplain).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: /Run Explain Analyze/ }));
    expect(onExplain).toHaveBeenCalledExactlyOnceWith(target, true);
  });

  it("does nothing when cancelled", async () => {
    const onExplain = vi.fn();
    render(<ExplainMenu disabled={false} getTarget={() => target} onExplain={onExplain} />);
    await openAnalyze();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(onExplain).not.toHaveBeenCalled();
  });

  it("runs plain EXPLAIN without confirmation", async () => {
    const onExplain = vi.fn();
    render(<ExplainMenu disabled={false} getTarget={() => target} onExplain={onExplain} />);
    fireEvent.click(screen.getByRole("button", { name: "More explain options" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: /^Explain estimates only/ }));
    expect(onExplain).toHaveBeenCalledExactlyOnceWith(target, false);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("ignores an empty statement", async () => {
    const onExplain = vi.fn();
    render(<ExplainMenu disabled={false} getTarget={() => ({ sql: "  ", offset: 0 })} onExplain={onExplain} />);
    fireEvent.click(screen.getByRole("button", { name: "More explain options" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: /Explain Analyze/ }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(onExplain).not.toHaveBeenCalled();
  });

  it("is disabled while a query runs", () => {
    render(<ExplainMenu disabled getTarget={() => target} onExplain={vi.fn()} />);
    expect((screen.getByRole("button", { name: "More explain options" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
