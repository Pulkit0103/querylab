import { describe, expect, it } from "vitest";

import { actionLabel, exportQuery, isFailure, summarize } from "@/lib/audit";

describe("audit helpers", () => {
  it("labels known actions and keeps unknown ones", () => {
    expect(actionLabel("auth.login")).toBe("Signed in");
    expect(actionLabel("custom.thing")).toBe("custom.thing");
  });

  it("flags failures", () => {
    expect(isFailure({ action: "auth.login_failed", detail: null })).toBe(true);
    expect(isFailure({ action: "query.executed", detail: { status: "sql_error" } })).toBe(true);
    expect(isFailure({ action: "query.executed", detail: { status: "success" } })).toBe(false);
  });

  it("summarizes details", () => {
    expect(summarize({ detail: { sql: "select\n  1", status: "success", rows: 1 } })).toBe("select 1 · status: success · rows: 1");
    expect(summarize({ detail: null })).toBe("");
  });

  it("builds export query strings", () => {
    expect(exportQuery({ action: "auth.", connection_id: "" })).toBe("?action=auth.");
    expect(exportQuery({})).toBe("");
  });
});
