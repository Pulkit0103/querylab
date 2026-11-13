import { describe, expect, it } from "vitest";

import { referencedTables } from "@/components/editor/sql-completion";
import type { CatalogEntry } from "@/hooks/use-schema";
import { errorMessageFor } from "@/lib/api-client";
import { safeNextPath } from "@/lib/auth";
import { compareCells } from "@/lib/compare-cells";
import { parseConnectionString } from "@/lib/connection-string";
import { toCsv, toTsv } from "@/lib/csv";
import { formatBytes, formatDuration } from "@/lib/format";
import { previewSql, qualifiedName, quoteIdent } from "@/lib/sql-ident";

describe("quoteIdent", () => {
  it("quotes only when necessary", () => {
    expect(quoteIdent("users")).toBe("users");
    expect(quoteIdent("Users")).toBe('"Users"');
    expect(quoteIdent("order")).toBe('"order"');
    expect(quoteIdent('we"ird')).toBe('"we""ird"');
    expect(qualifiedName("public", "t")).toBe("t");
    expect(qualifiedName("sales", "Orders")).toBe('sales."Orders"');
    expect(previewSql("sales", "orders", 10)).toContain("FROM sales.orders\nLIMIT 10");
  });
});

describe("csv", () => {
  const columns = [
    { name: "a", type: "text" },
    { name: "b", type: "int4" },
  ];

  it("escapes quotes, commas and newlines", () => {
    expect(toCsv(columns, [['x,"y"', 1], ["line\nbreak", null]])).toBe('a,b\r\n"x,""y""",1\r\n"line\nbreak",');
  });

  it("neutralises spreadsheet formulas", () => {
    expect(toCsv(columns, [["=HYPERLINK(1)", 2]])).toBe("a,b\r\n'=HYPERLINK(1),2");
  });

  it("produces TSV for clipboard", () => {
    expect(toTsv([["a\tb", { k: 1 }, null]])).toBe('a b\t{"k":1}\t');
  });
});

describe("compareCells", () => {
  it("sorts numerically, nulls last", () => {
    const values = ["10", null, "9", "100.5"];
    expect([...values].sort((a, b) => compareCells(a, b, true))).toEqual(["9", "10", "100.5", null]);
  });

  it("sorts text naturally", () => {
    expect(["b2", "b10", "a"].sort((a, b) => compareCells(a, b, false))).toEqual(["a", "b2", "b10"]);
  });
});

describe("parseConnectionString", () => {
  it("parses postgres URLs", () => {
    expect(parseConnectionString("postgresql://us%40r:p%3Ass@db.example.com:6543/app?sslmode=verify-full")).toEqual({
      host: "db.example.com",
      port: 6543,
      database: "app",
      username: "us@r",
      password: "p:ss",
      ssl_mode: "verify-full",
    });
  });

  it("defaults remote hosts to sslmode=require", () => {
    expect(parseConnectionString("postgres://u@remote.example.com/db")?.ssl_mode).toBe("require");
    expect(parseConnectionString("postgres://u@localhost/db")?.ssl_mode).toBeUndefined();
  });

  it("rejects non-postgres input", () => {
    expect(parseConnectionString("mysql://x")).toBeNull();
    expect(parseConnectionString("hello")).toBeNull();
  });
});

describe("safeNextPath", () => {
  it("prevents open redirects", () => {
    expect(safeNextPath("/explorer?x=1")).toBe("/explorer?x=1");
    expect(safeNextPath("//evil.com")).toBe("/editor");
    expect(safeNextPath("https://evil.com")).toBe("/editor");
    expect(safeNextPath("/\\evil.com")).toBe("/editor");
    expect(safeNextPath("/login")).toBe("/editor");
    expect(safeNextPath(null)).toBe("/editor");
  });
});

describe("format", () => {
  it("formats durations and sizes", () => {
    expect(formatDuration(0.4)).toBe("<1 ms");
    expect(formatDuration(850)).toBe("850 ms");
    expect(formatDuration(1240)).toBe("1.24 s");
    expect(formatDuration(125_000)).toBe("2m 05s");
    expect(formatBytes(1536)).toBe("1.5 KB");
  });
});

describe("referencedTables", () => {
  const catalog: CatalogEntry[] = [
    { schema: "public", table: "employees", kind: "table", columns: ["id", "name"], column_types: ["int4", "text"] },
    { schema: "sales", table: "orders", kind: "table", columns: ["id", "total"], column_types: ["int8", "numeric"] },
  ];

  it("resolves tables and aliases from FROM/JOIN clauses", () => {
    const map = referencedTables("select * from employees e join sales.orders as o on o.id = e.id where", catalog);
    expect(map.get("e")?.table).toBe("employees");
    expect(map.get("o")?.table).toBe("orders");
    expect(map.get("orders")?.schema).toBe("sales");
  });

  it("does not treat keywords as aliases", () => {
    const map = referencedTables("select * from employees where id = 1", catalog);
    expect(map.has("where")).toBe(false);
    expect(map.get("employees")).toBeDefined();
  });
});

describe("errorMessageFor", () => {
  it("adds the request id to server errors only", () => {
    expect(errorMessageFor(500, "internal server error", "abc-123")).toBe("internal server error (reference abc-123)");
    expect(errorMessageFor(502, "bad gateway", null)).toBe("bad gateway");
    expect(errorMessageFor(404, "connection not found", "abc-123")).toBe("connection not found");
  });
});
