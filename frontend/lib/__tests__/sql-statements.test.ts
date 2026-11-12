import { describe, expect, it } from "vitest";

import { hasContent, splitStatements, statementAt, trimStatement } from "@/lib/sql-statements";

const texts = (sql: string) => splitStatements(sql).map((s) => s.text.trim()).filter(Boolean);

describe("splitStatements", () => {
  it("splits on top-level semicolons", () => {
    expect(texts("select 1; select 2;select 3")).toEqual(["select 1", "select 2", "select 3"]);
  });

  it("ignores semicolons in strings, identifiers and comments", () => {
    const sql = `select 'a;b', "we;ird" from t -- c;omment
      ; select /* x; /* nested; */ y */ 2; select E'it\\'s;' ; select 'don''t;'`;
    expect(texts(sql)).toEqual([
      `select 'a;b', "we;ird" from t -- c;omment`,
      "select /* x; /* nested; */ y */ 2",
      "select E'it\\'s;'",
      "select 'don''t;'",
    ]);
  });

  it("handles dollar quoting", () => {
    const sql = "create function f() returns int as $$ select 1; $$ language sql; select $tag$ a;b $tag$";
    expect(texts(sql)).toEqual([
      "create function f() returns int as $$ select 1; $$ language sql",
      "select $tag$ a;b $tag$",
    ]);
  });

  it("does not treat positional parameters as dollar quotes", () => {
    expect(texts("select $1; select 2")).toEqual(["select $1", "select 2"]);
  });

  it("reports offsets that map back to the source", () => {
    const sql = "select 1;\n\nselect 2;";
    const [, second] = splitStatements(sql);
    const { text, offset } = trimStatement(second);
    expect(text).toBe("select 2");
    expect(sql.slice(offset, offset + text.length)).toBe("select 2");
  });
});

describe("statementAt", () => {
  const sql = "select 1;\n-- only a comment;\nselect 2;\n\n";

  it("returns the statement containing the cursor", () => {
    expect(statementAt(sql, 3)?.text.trim()).toBe("select 1");
    expect(statementAt(sql, sql.indexOf("select 2") + 2)?.text.trim()).toContain("select 2");
  });

  it("uses the previous statement when the cursor is in trailing whitespace", () => {
    expect(statementAt(sql, sql.length)?.text).toContain("select 2");
  });

  it("skips comment-only fragments", () => {
    expect(hasContent("-- only a comment")).toBe(false);
    expect(hasContent("/* c */ select 1")).toBe(true);
  });

  it("returns null for empty input", () => {
    expect(statementAt("   ", 0)).toBeNull();
  });
});
