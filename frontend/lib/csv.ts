import type { CellValue, ColumnMeta } from "@/types/query";

function escapeCsv(value: CellValue): string {
  if (value === null || value === undefined) return "";
  const text = typeof value === "object" ? JSON.stringify(value) : String(value);
  // Neutralise spreadsheet formula injection (CWE-1236).
  const safe = /^[=+\-@\t\r]/.test(text) ? `'${text}` : text;
  return /[",\r\n]/.test(safe) ? `"${safe.replace(/"/g, '""')}"` : safe;
}

/** Serialises a result set to RFC 4180 CSV. */
export function toCsv(columns: ColumnMeta[], rows: CellValue[][]): string {
  const header = columns.map((c) => escapeCsv(c.name)).join(",");
  const body = rows.map((row) => row.map(escapeCsv).join(","));
  return [header, ...body].join("\r\n");
}

/** Tab-separated text suitable for pasting into spreadsheets. */
export function toTsv(rows: CellValue[][]): string {
  return rows
    .map((row) =>
      row
        .map((v) => (v === null ? "" : typeof v === "object" ? JSON.stringify(v) : String(v)))
        .map((s) => s.replace(/[\t\r\n]+/g, " "))
        .join("\t"),
    )
    .join("\n");
}

export function downloadText(filename: string, content: string, mime = "text/csv;charset=utf-8") {
  const blob = new Blob(["﻿", content], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
