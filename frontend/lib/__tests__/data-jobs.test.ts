import { describe, expect, it } from "vitest";

import {
  chunkRanges,
  defaultGenerator,
  describeGenerator,
  errorsCsv,
  issuesFor,
  isFinished,
  progressPercent,
  setMapping,
  splitQualified,
  withParents,
} from "@/lib/data-jobs";

describe("data job helpers", () => {
  it("splits files into chunks", () => {
    expect(chunkRanges(10, 4)).toEqual([
      [0, 4],
      [4, 8],
      [8, 10],
    ]);
    expect(chunkRanges(0, 4)).toEqual([]);
  });

  it("reports progress", () => {
    expect(progressPercent({ status: "running", rows_total: 200, rows_processed: 50 })).toBe(25);
    expect(progressPercent({ status: "running", rows_total: null, rows_processed: 50 })).toBeNull();
    expect(progressPercent({ status: "completed", rows_total: null, rows_processed: 0 })).toBe(100);
    expect(isFinished("running")).toBe(false);
    expect(isFinished("cancelled")).toBe(true);
  });

  it("edits mappings", () => {
    const m = setMapping([{ source: 0, target: "a" }], "b", 2);
    expect(m).toEqual([
      { source: 0, target: "a" },
      { source: 2, target: "b" },
    ]);
    expect(setMapping(m, "a", null)).toEqual([{ source: 2, target: "b" }]);
    expect(setMapping(m, "a", 3)).toContainEqual({ source: 3, target: "a" });
  });

  it("finds the issues of a column", () => {
    const issues = [
      { severity: "error" as const, source: null, target: "email", message: "required" },
      { severity: "warning" as const, source: 1, target: "age", message: "decimals" },
      { severity: "error" as const, source: 4, target: null, message: "missing source" },
    ];
    expect(issuesFor(issues, "email", null).map((i) => i.message)).toEqual(["required"]);
    expect(issuesFor(issues, null, 4).map((i) => i.message)).toEqual(["missing source"]);
  });

  it("writes rejected rows as CSV", () => {
    expect(errorsCsv([{ row: 2, line: 3, column: "age", message: 'bad "value", sorry' }])).toBe(
      'row,line,column,message\r\n2,3,age,"bad ""value"", sorry"\r\n',
    );
  });

  it("adds missing parents ahead of their children", () => {
    expect(splitQualified('sales."Order Items"')).toEqual(["sales", "Order Items"]);
    expect(splitQualified("a.b.c")).toBeNull();
    const tables = withParents([{ schema: "sales", table: "orders", rows: 500 }], ["sales.customers", "sales.orders"]);
    expect(tables).toEqual([
      { schema: "sales", table: "customers", rows: 100 },
      { schema: "sales", table: "orders", rows: 500 },
    ]);
  });

  it("builds and describes generators", () => {
    const now = new Date("2026-06-15T00:00:00Z");
    expect(defaultGenerator("date", now)).toEqual({ kind: "date", from: "2025-06-15", to: "2026-06-15" });
    expect(describeGenerator({ kind: "integer", min: 1, max: 5 })).toBe("1 – 5");
    expect(describeGenerator({ kind: "sequence", start: null })).toBe("after the current maximum");
    expect(describeGenerator({ kind: "boolean", true_ratio: 0.25 })).toBe("25% true");
  });
});
