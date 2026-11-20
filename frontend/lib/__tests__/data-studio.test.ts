import { describe, expect, it } from "vitest";

import {
  addInsert,
  changeCount,
  changeTarget,
  countConditions,
  countFilterConditions,
  emptyChanges,
  newCondition,
  newGroup,
  operatorsFor,
  removeInsert,
  rowRef,
  rowsToTsv,
  setInsertValue,
  stageEdit,
  tableEditorHref,
  toApiFilter,
  toChangeSet,
  toggleDelete,
  updateNode,
  type UiCondition,
} from "@/lib/data-studio";
import type { DataColumn } from "@/types/data";

const column = (name: string, extra: Partial<DataColumn> = {}): DataColumn => ({
  name,
  data_type: "text",
  type_name: "text",
  kind: "text",
  nullable: true,
  has_default: false,
  is_key: false,
  identity: null,
  generated: false,
  editable: true,
  enum_values: [],
  ...extra,
});

const columns = [column("id", { is_key: true, kind: "number" }), column("name"), column("age", { kind: "number" })];

describe("row identity", () => {
  it("builds a stable id from key values", () => {
    const ref = rowRef(columns, ["id"], ["7", "Ada", "36"]);
    expect(ref).toEqual({ id: '["7"]', key: { id: "7" } });
    expect(rowRef(columns, [], ["7", "Ada", "36"])).toBeNull();
    expect(rowRef(columns, ["missing"], ["7"])).toBeNull();
  });
});

describe("staged changes", () => {
  const ref = rowRef(columns, ["id"], ["7", "Ada", "36"])!;

  it("stages edits and drops them when the original value is restored", () => {
    let s = stageEdit(emptyChanges(), ref, "age", "37", "36");
    s = stageEdit(s, ref, "name", null, "Ada");
    expect(s.edits[ref.id]).toEqual({ key: { id: "7" }, values: { age: "37", name: null }, original: { age: "36", name: "Ada" } });
    s = stageEdit(s, ref, "age", "36", "36");
    expect(s.edits[ref.id].values).toEqual({ name: null });
    s = stageEdit(s, ref, "name", "Ada", "Ada");
    expect(s.edits).toEqual({});
  });

  it("deleting a row discards its edits; toggling again restores it", () => {
    let s = stageEdit(emptyChanges(), ref, "age", "37", "36");
    s = toggleDelete(s, ref);
    expect(s.deletes[ref.id]).toEqual({ id: "7" });
    expect(s.edits).toEqual({});
    s = toggleDelete(s, ref);
    expect(changeCount(s)).toBe(0);
  });

  it("tracks inserts, defaults and the API payload", () => {
    let s = addInsert(emptyChanges(), "n1");
    s = addInsert(s, "n2");
    s = setInsertValue(s, "n1", "name", "Grace");
    s = setInsertValue(s, "n1", "age", null);
    s = setInsertValue(s, "n1", "age", undefined); // back to the default
    s = stageEdit(s, ref, "age", "37", "36");
    s = toggleDelete(s, rowRef(columns, ["id"], ["9", "x", null])!);
    expect(s.inserts.map((r) => r.id)).toEqual(["n2", "n1"]);
    expect(changeCount(s)).toBe(4);
    expect(toChangeSet(s)).toEqual({
      inserts: [{ values: {} }, { values: { name: "Grace" } }],
      updates: [{ key: { id: "7" }, values: { age: "37" }, original: { age: "36" } }],
      deletes: [{ key: { id: "9" } }],
    });
    expect(changeTarget(s, "insert", 1)).toEqual({ insertId: "n1" });
    expect(changeTarget(s, "update", 0)).toEqual({ rowId: ref.id });
    expect(changeTarget(s, "delete", 0)).toEqual({ rowId: '["9"]' });
    expect(changeTarget(s, null, null)).toEqual({});
    expect(removeInsert(s, "n2").inserts).toHaveLength(1);
  });
});

describe("filter model", () => {
  const cond = (patch: Partial<UiCondition>): UiCondition => ({ ...newCondition(), ...patch });

  it("converts conditions by operator arity and skips incomplete rows", () => {
    const group = newGroup("and", [
      cond({ column: "age", operator: ">", value: "25" }),
      cond({ column: "name", operator: "in", value: "a, b,, c " }),
      cond({ column: "age", operator: "between", value: "1", value2: "9" }),
      cond({ column: "name", operator: "is_null", value: "ignored" }),
      cond({ column: "", operator: "=", value: "x" }),
      cond({ column: "name", operator: "in", value: " , " }),
      newGroup("or", [cond({ column: "name", operator: "ilike", value: "%a%" }), newGroup("and")]),
    ]);
    expect(toApiFilter(group)).toEqual({
      op: "and",
      items: [
        { column: "age", operator: ">", value: "25" },
        { column: "name", operator: "in", value: ["a", "b", "c"] },
        { column: "age", operator: "between", value: ["1", "9"] },
        { column: "name", operator: "is_null" },
        { op: "or", items: [{ column: "name", operator: "ilike", value: "%a%" }] },
      ],
    });
    expect(countConditions(group)).toBe(6);
    expect(countFilterConditions(toApiFilter(group))).toBe(5);
    expect(toApiFilter(newGroup("and", [newCondition()]))).toBeNull();
    expect(countFilterConditions(null)).toBe(0);
  });

  it("updates and removes nested nodes", () => {
    const inner = cond({ column: "a" });
    const sub = newGroup("or", [inner]);
    const root = newGroup("and", [cond({ column: "b" }), sub]);
    const renamed = updateNode(root, inner.id, (n) => ({ ...(n as UiCondition), column: "z" }));
    expect(((renamed.items[1] as typeof sub).items[0] as UiCondition).column).toBe("z");
    expect(updateNode(root, sub.id, () => null).items).toHaveLength(1);
  });

  it("offers operators that suit the column kind", () => {
    expect(operatorsFor("boolean")).toEqual(["=", "!=", "is_null", "is_not_null"]);
    expect(operatorsFor("number")).toContain("between");
    expect(operatorsFor("enum")).toContain("in");
    expect(operatorsFor("json")).not.toContain(">");
  });
});

describe("helpers", () => {
  it("builds editor links and TSV", () => {
    expect(tableEditorHref("my schema", "t&1")).toBe("/tables?schema=my+schema&table=t%261");
    expect(rowsToTsv(["a", "b"], [["1\t2", null]])).toBe("a\tb\n1 2\t");
  });
});
