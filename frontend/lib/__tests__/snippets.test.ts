import { describe, expect, it } from "vitest";

import { fillSnippet, placeholders } from "@/lib/snippets";

describe("snippet placeholders", () => {
  it("lists placeholders once, in order", () => {
    expect(placeholders("select * from {{table}} where id = {{ id }} and t = '{{table}}' {{}} {{bad!}}")).toEqual(["table", "id"]);
    expect(placeholders("no placeholders {{ unterminated")).toEqual([]);
  });

  it("fills known placeholders and keeps the rest", () => {
    expect(fillSnippet("select * from {{ table }} limit {{n}} -- {{other}}", { table: "users", n: "10" })).toBe(
      "select * from users limit 10 -- {{other}}",
    );
  });
});
