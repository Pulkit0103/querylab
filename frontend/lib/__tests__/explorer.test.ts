import { describe, expect, it } from "vitest";

import { extrasBySchema, functionSignature, sameObject } from "@/lib/explorer";
import { insertTemplate, selectSql } from "@/lib/sql-ident";
import type { FunctionInfo } from "@/types/schema";

const fn = (schema: string, name: string, oid: number): FunctionInfo => ({
  oid,
  schema,
  name,
  kind: "function",
  arguments: "x integer",
  result: "integer",
  language: "sql",
  volatility: "immutable",
  security_definer: false,
  comment: null,
});

describe("SQL generators", () => {
  it("builds SELECT with quoted explicit columns", () => {
    expect(selectSql("sales", "orders", ["id", "Total", "select"])).toBe(
      'SELECT id,\n       "Total",\n       "select"\nFROM sales.orders\nLIMIT 100;\n',
    );
    expect(selectSql("public", "t", [])).toBe("SELECT *\nFROM t\nLIMIT 100;\n");
  });

  it("builds INSERT templates with typed placeholders, defaulted columns last", () => {
    const sql = insertTemplate("inventory", "products", [
      { name: "id", data_type: "bigint", default_value: "nextval(...)" },
      { name: "name", data_type: "text", default_value: null },
      { name: "price", data_type: "numeric(10,2)", default_value: null },
    ]);
    expect(sql).toBe(
      "INSERT INTO inventory.products (name, price, id)\nVALUES (\n" +
        "  NULL /* name text */,\n  NULL /* price numeric(10,2) */,\n  NULL /* id bigint, has default */\n);\n",
    );
  });
});

describe("explorer helpers", () => {
  it("groups functions, sequences and types by schema", () => {
    const map = extrasBySchema([fn("a", "f1", 1), fn("b", "f2", 2), fn("a", "f3", 3)], {
      sequences: [{ schema: "b", name: "s", data_type: "bigint", start_value: 1, increment_by: 1, last_value: null, owned_by: null }],
      types: [{ schema: "a", name: "t", kind: "enum", detail: null, values: ["x"], comment: null }],
    });
    expect(map.get("a")?.functions.map((f) => f.name)).toEqual(["f1", "f3"]);
    expect(map.get("a")?.types).toHaveLength(1);
    expect(map.get("b")?.sequences).toHaveLength(1);
    expect(extrasBySchema(undefined, undefined).size).toBe(0);
  });

  it("compares selections and formats signatures", () => {
    expect(sameObject({ kind: "function", oid: 1, schema: "a", name: "f" }, { kind: "function", oid: 1, schema: "x", name: "y" })).toBe(true);
    expect(sameObject({ kind: "type", schema: "a", name: "t" }, { kind: "sequence", schema: "a", name: "t" })).toBe(false);
    expect(sameObject(null, { kind: "type", schema: "a", name: "t" })).toBe(false);
    expect(functionSignature(fn("a", "add_one", 1))).toBe("add_one(x integer)");
  });
});
