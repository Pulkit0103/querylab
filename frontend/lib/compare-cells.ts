import { cellToText } from "@/lib/format";
import type { CellValue } from "@/types/query";

/** PostgreSQL types sorted and aligned numerically in the results grid. */
export const NUMERIC_TYPES = new Set(["int2", "int4", "int8", "float4", "float8", "numeric", "oid", "money"]);

/**
 * Sort comparator for result cells. NULLs sort last; numeric columns compare
 * numerically even when values arrive as strings (numeric/int8 precision).
 */
export function compareCells(a: CellValue, b: CellValue, numeric: boolean): number {
  if (a === null && b === null) return 0;
  if (a === null) return 1;
  if (b === null) return -1;
  if (numeric || (typeof a === "number" && typeof b === "number")) {
    const na = Number(a);
    const nb = Number(b);
    if (!Number.isNaN(na) && !Number.isNaN(nb)) return na - nb;
  }
  if (typeof a === "boolean" && typeof b === "boolean") return Number(a) - Number(b);
  return cellToText(a).localeCompare(cellToText(b), undefined, { numeric: true, sensitivity: "base" });
}
