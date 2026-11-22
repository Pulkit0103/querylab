import { describe, expect, it } from "vitest";

import { buildDownScript, buildScript, defaultSelection, groupChanges, isManual, migrationName, summarize } from "@/lib/schema-diff";
import type { DiffResponse, SchemaChange } from "@/types/migrations";

const change = (c: Partial<SchemaChange>): SchemaChange => ({
  phase: "create_table",
  object_type: "table",
  action: "create",
  object: "t",
  sql: "CREATE TABLE s.t ();",
  down_sql: "DROP TABLE s.t;",
  destructive: false,
  notes: [],
  before: null,
  after: null,
  ...c,
});

const diff: DiffResponse = {
  source_schema: "src",
  target_schema: "s",
  preamble: "SET LOCAL search_path = s;",
  hints: [],
  changes: [
    change({ phase: "drop_column", object_type: "column", action: "drop", object: "t.old", sql: "ALTER TABLE s.t DROP COLUMN old;", down_sql: null, destructive: true }),
    change({ object: "t" }),
    change({ phase: "create_index", object_type: "index", object: "t.i", sql: "CREATE INDEX i ON t (a);", down_sql: "DROP INDEX s.i;" }),
    change({ phase: "create_type", object_type: "type", action: "alter", object: "mood", sql: "-- Enum s.mood lost values", down_sql: null, destructive: true }),
  ],
};

describe("schema diff helpers", () => {
  it("selects safe changes by default", () => {
    expect([...defaultSelection(diff.changes)]).toEqual([1, 2]);
    expect(isManual(diff.changes[3])).toBe(true);
  });

  it("groups changes by object type in a stable order", () => {
    expect(groupChanges(diff.changes).map((g) => `${g.label}:${g.items.join(",")}`)).toEqual([
      "Types:3",
      "Tables:1",
      "Columns:0",
      "Indexes:2",
    ]);
    expect(summarize(diff.changes)).toEqual({ create: 2, alter: 1, drop: 1, destructive: 2 });
  });

  it("builds up and down scripts from the selection", () => {
    expect(buildScript(diff, new Set([1, 2]))).toBe(
      "SET LOCAL search_path = s;\n\nCREATE TABLE s.t ();\n\nCREATE INDEX i ON t (a);\n",
    );
    expect(buildScript(diff, new Set())).toBe("");
    const down = buildDownScript(diff, new Set([0, 1, 2]));
    expect(down.sql).toBe(
      "SET LOCAL search_path = s;\n\nDROP INDEX s.i;\n\nDROP TABLE s.t;\n\n-- Not reversible: t.old\n",
    );
    expect(down.missing).toEqual(["t.old"]);
    expect(migrationName(diff, "dev.src")).toBe("Sync s with dev.src");
  });
});
