/** Explorer selection model and grouping helpers. */

import type { FunctionInfo, SchemaObjects, SequenceInfo, TypeInfo } from "@/types/schema";

/** A non-table object selected in the explorer (tables use the workspace store). */
export type ExplorerObject =
  | { kind: "function"; oid: number; schema: string; name: string }
  | { kind: "sequence"; schema: string; name: string }
  | { kind: "type"; schema: string; name: string };

export function sameObject(a: ExplorerObject | null, b: ExplorerObject): boolean {
  if (!a || a.kind !== b.kind) return false;
  if (a.kind === "function" && b.kind === "function") return a.oid === b.oid;
  return a.schema === b.schema && a.name === b.name;
}

export interface SchemaExtras {
  functions: FunctionInfo[];
  sequences: SequenceInfo[];
  types: TypeInfo[];
}

/** Buckets functions, sequences and types by schema name. */
export function extrasBySchema(
  functions: FunctionInfo[] | undefined,
  objects: SchemaObjects | undefined,
): Map<string, SchemaExtras> {
  const map = new Map<string, SchemaExtras>();
  const get = (schema: string) => {
    let entry = map.get(schema);
    if (!entry) {
      entry = { functions: [], sequences: [], types: [] };
      map.set(schema, entry);
    }
    return entry;
  };
  for (const f of functions ?? []) get(f.schema).functions.push(f);
  for (const s of objects?.sequences ?? []) get(s.schema).sequences.push(s);
  for (const t of objects?.types ?? []) get(t.schema).types.push(t);
  return map;
}

/** `name(args)` for display. */
export function functionSignature(f: Pick<FunctionInfo, "name" | "arguments">): string {
  return `${f.name}(${f.arguments})`;
}
