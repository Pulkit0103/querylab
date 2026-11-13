// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { PlanView } from "@/components/results/plan-view";
import type { TabRuntime } from "@/stores/editor-store";
import type { ExplainResult } from "@/types/query";

afterEach(cleanup);

const analyzed: ExplainResult = {
  analyzed: true,
  planning_time_ms: 0.4,
  execution_time_ms: 12.5,
  raw: [{ Plan: { "Node Type": "Hash Join" } }],
  plan: {
    "Node Type": "Hash Join",
    "Join Type": "Inner",
    "Total Cost": 100,
    "Plan Rows": 10,
    "Actual Total Time": 10,
    "Actual Rows": 1000,
    "Actual Loops": 1,
    "Hash Cond": "(o.customer_id = c.id)",
    Plans: [
      {
        "Node Type": "Seq Scan",
        "Relation Name": "orders",
        Schema: "sales",
        Alias: "o",
        "Total Cost": 80,
        "Plan Rows": 1000,
        "Actual Total Time": 8,
        "Actual Rows": 1000,
        "Actual Loops": 1,
      },
      {
        "Node Type": "Hash",
        "Total Cost": 5,
        "Plan Rows": 10,
        "Actual Total Time": 0.5,
        "Actual Rows": 10,
        "Actual Loops": 1,
        Plans: [
          {
            "Node Type": "Index Scan",
            "Relation Name": "customers",
            "Index Name": "customers_pkey",
            "Total Cost": 4,
            "Plan Rows": 10,
            "Actual Total Time": 0.4,
            "Actual Rows": 10,
            "Actual Loops": 1,
          },
        ],
      },
    ],
  },
};

function runtime(overrides: Partial<TabRuntime> = {}): TabRuntime {
  return { exec: { status: "idle" }, plan: null, planError: null, messages: [], view: "plan", ...overrides };
}

describe("PlanView", () => {
  it("prompts to run Explain when there is no plan", () => {
    render(<PlanView runtime={runtime()} />);
    expect(screen.getByText("No execution plan")).toBeTruthy();
  });

  it("shows explain errors", () => {
    render(<PlanView runtime={runtime({ planError: { code: "sql_error", message: "relation \"nope\" does not exist" } })} />);
    expect(screen.getByText("Could not explain query")).toBeTruthy();
    expect(screen.getByText(/relation "nope" does not exist/)).toBeTruthy();
  });

  it("summarises an analyzed plan and highlights hot nodes and misestimates", () => {
    render(<PlanView runtime={runtime({ plan: analyzed })} />);
    expect(screen.getByText("Explain analyze")).toBeTruthy();
    expect(screen.getByText("13 ms")).toBeTruthy(); // execution time
    expect(screen.getByRole("button", { name: /Slowest: Seq Scan \(80% of time\)/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: /Worst estimate: 100× on Hash Join/ })).toBeTruthy();
    expect(screen.getByText("on sales.orders o")).toBeTruthy();
    expect(screen.getByText("using customers_pkey")).toBeTruthy();
    expect(screen.getByText("100× under")).toBeTruthy();
    // The hottest node starts expanded with its details.
    expect(screen.getByText("Actual time")).toBeTruthy();
  });

  it("collapses children, toggles details and switches to raw JSON", () => {
    render(<PlanView runtime={runtime({ plan: analyzed })} />);
    expect(screen.getByText("Index Scan")).toBeTruthy();
    const [rootToggle] = screen.getAllByRole("button", { name: "Collapse children" });
    fireEvent.click(rootToggle);
    expect(screen.queryByText("Index Scan")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Expand children" }));
    expect(screen.getByText("Index Scan")).toBeTruthy();

    fireEvent.click(screen.getByText("Hash Join"));
    expect(screen.getByText("(o.customer_id = c.id)")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "JSON" }));
    expect(screen.getByText(/"Node Type": "Hash Join"/)).toBeTruthy();
    expect(screen.queryByRole("list", { name: "Execution plan" })).toBeNull();
  });

  it("labels plain EXPLAIN as estimates with cost shares", () => {
    const plain: ExplainResult = {
      analyzed: false,
      planning_time_ms: null,
      execution_time_ms: null,
      raw: [],
      plan: { "Node Type": "Seq Scan", "Relation Name": "t", "Total Cost": 12.5, "Plan Rows": 40 },
    };
    render(<PlanView runtime={runtime({ plan: plain })} />);
    expect(screen.getByText("Explain · estimates")).toBeTruthy();
    expect(screen.queryByText("Execution")).toBeNull();
    expect(screen.getByText("est")).toBeTruthy();
  });
});
