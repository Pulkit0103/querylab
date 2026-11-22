/** Pure helpers for imports, mock data and the job log. */
import { ApiError } from "@/lib/api-client";
import type {
  ColumnMapping,
  Generator,
  GeneratorKind,
  InferredType,
  JobStatus,
  JobView,
  MappingIssue,
  MockTable,
  RowError,
} from "@/types/data-jobs";

export const ACTIVE_STATUSES: JobStatus[] = ["uploading", "ready", "running"];
export const isFinished = (s: JobStatus) => !ACTIVE_STATUSES.includes(s);

/** `[start, end)` byte ranges of the chunks of a file. */
export function chunkRanges(size: number, chunk: number): [number, number][] {
  const ranges: [number, number][] = [];
  for (let start = 0; start < size; start += chunk) ranges.push([start, Math.min(size, start + chunk)]);
  return ranges;
}

/** 0–100, or null when the total is unknown. */
export function progressPercent(view: Pick<JobView, "status" | "rows_total" | "rows_processed">): number | null {
  if (view.status === "completed") return 100;
  if (!view.rows_total) return null;
  return Math.min(100, Math.round((view.rows_processed / view.rows_total) * 100));
}

export const TYPE_LABEL: Record<InferredType, string> = {
  unknown: "empty",
  boolean: "boolean",
  integer: "integer",
  decimal: "decimal",
  date: "date",
  timestamp: "timestamp",
  uuid: "uuid",
  json: "json",
  text: "text",
};

/** Issues about one source column or one target column. */
export function issuesFor(issues: MappingIssue[], target: string | null, source: number | null): MappingIssue[] {
  return issues.filter((i) => (target !== null && i.target === target) || (source !== null && i.source === source && i.target === null));
}

/** Sets (or clears, with `source === null`) the source of a target column. */
export function setMapping(mapping: ColumnMapping[], target: string, source: number | null): ColumnMapping[] {
  const rest = mapping.filter((m) => m.target !== target);
  return source === null ? rest : [...rest, { source, target }];
}

function csvField(v: string | number | null): string {
  if (v === null) return "";
  const s = String(v);
  return /[",\r\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/** The rejected rows of a report as CSV. */
export function errorsCsv(errors: RowError[]): string {
  const lines = [["row", "line", "column", "message"].join(",")];
  for (const e of errors) lines.push([e.row, e.line, e.column, e.message].map(csvField).join(","));
  return lines.join("\r\n") + "\r\n";
}

/** Splits `schema.table` where either part may be a quoted identifier. */
export function splitQualified(name: string): [string, string] | null {
  const parts: string[] = [];
  let i = 0;
  while (i < name.length) {
    if (name[i] === '"') {
      let part = "";
      i++;
      while (i < name.length) {
        if (name[i] === '"' && name[i + 1] === '"') {
          part += '"';
          i += 2;
        } else if (name[i] === '"') {
          i++;
          break;
        } else part += name[i++];
      }
      parts.push(part);
    } else {
      const end = name.indexOf(".", i);
      parts.push(name.slice(i, end === -1 ? undefined : end));
      i = end === -1 ? name.length : end;
    }
    if (name[i] === ".") i++;
  }
  return parts.length === 2 ? [parts[0], parts[1]] : null;
}

/** Adds parent tables the run needs, with the same row count as the first table. */
export function withParents(tables: MockTable[], missing: string[]): MockTable[] {
  const rows = Math.max(1, Math.min(...tables.map((t) => t.rows).filter((n) => n > 0), 100));
  const added = missing
    .map(splitQualified)
    .filter((p): p is [string, string] => p !== null)
    .filter(([s, t]) => !tables.some((x) => x.schema === s && x.table === t))
    .map(([schema, table]) => ({ schema, table, rows }));
  return [...added, ...tables];
}

export interface GeneratorOption {
  kind: GeneratorKind;
  label: string;
  group: "Basics" | "People" | "Text" | "Places" | "Numbers" | "Dates" | "Other";
}

export const GENERATORS: GeneratorOption[] = [
  { kind: "default", label: "Database default", group: "Basics" },
  { kind: "null", label: "NULL", group: "Basics" },
  { kind: "constant", label: "Constant", group: "Basics" },
  { kind: "values", label: "One of a list", group: "Basics" },
  { kind: "first_name", label: "First name", group: "People" },
  { kind: "last_name", label: "Last name", group: "People" },
  { kind: "full_name", label: "Full name", group: "People" },
  { kind: "email", label: "Email", group: "People" },
  { kind: "username", label: "Username", group: "People" },
  { kind: "phone", label: "Phone", group: "People" },
  { kind: "company", label: "Company", group: "Text" },
  { kind: "product_name", label: "Product name", group: "Text" },
  { kind: "word", label: "Word", group: "Text" },
  { kind: "sentence", label: "Sentence", group: "Text" },
  { kind: "paragraph", label: "Paragraph", group: "Text" },
  { kind: "url", label: "URL", group: "Text" },
  { kind: "city", label: "City", group: "Places" },
  { kind: "country", label: "Country", group: "Places" },
  { kind: "country_code", label: "Country code", group: "Places" },
  { kind: "street_address", label: "Street address", group: "Places" },
  { kind: "postal_code", label: "Postal code", group: "Places" },
  { kind: "integer", label: "Integer", group: "Numbers" },
  { kind: "sequence", label: "Sequence", group: "Numbers" },
  { kind: "decimal", label: "Decimal", group: "Numbers" },
  { kind: "boolean", label: "Boolean", group: "Numbers" },
  { kind: "date", label: "Date", group: "Dates" },
  { kind: "timestamp", label: "Timestamp", group: "Dates" },
  { kind: "uuid", label: "UUID", group: "Other" },
  { kind: "json", label: "JSON object", group: "Other" },
  { kind: "reference", label: "Foreign key", group: "Other" },
];

export const generatorLabel = (kind: GeneratorKind) => GENERATORS.find((g) => g.kind === kind)?.label ?? kind;

const isoDate = (d: Date) => d.toISOString().slice(0, 10);

/** A generator of `kind` with sensible parameters. */
export function defaultGenerator(kind: GeneratorKind, now = new Date()): Generator {
  const yearAgo = new Date(now);
  yearAgo.setFullYear(now.getFullYear() - 1);
  switch (kind) {
    case "constant":
      return { kind, value: "" };
    case "values":
      return { kind, values: ["a", "b"] };
    case "boolean":
      return { kind, true_ratio: 0.5 };
    case "integer":
      return { kind, min: 1, max: 1000 };
    case "sequence":
      return { kind, start: null };
    case "decimal":
      return { kind, min: 1, max: 1000, scale: 2 };
    case "date":
    case "timestamp":
      return { kind, from: isoDate(yearAgo), to: isoDate(now) };
    default:
      return { kind } as Generator;
  }
}

/** One-line description of a generator's parameters. */
export function describeGenerator(g: Generator): string {
  switch (g.kind) {
    case "constant":
      return g.value === "" ? "empty string" : `“${g.value}”`;
    case "values":
      return g.values.join(", ");
    case "boolean":
      return `${Math.round(g.true_ratio * 100)}% true`;
    case "integer":
    case "decimal":
      return `${g.min} – ${g.max}`;
    case "sequence":
      return g.start == null ? "after the current maximum" : `from ${g.start}`;
    case "date":
    case "timestamp":
      return `${g.from} – ${g.to}`;
    default:
      return "";
  }
}

/** Uploads a file in chunks; resumes from the server's offset after a conflict. */
export async function uploadChunks(
  importId: string,
  file: Blob,
  chunkBytes: number,
  onProgress: (sent: number) => void,
  signal?: AbortSignal,
): Promise<void> {
  let offset = 0;
  let attempts = 0;
  while (offset < file.size) {
    const end = Math.min(file.size, offset + chunkBytes);
    let res: Response;
    try {
      res = await fetch(`/api/imports/${importId}/data?offset=${offset}`, {
        method: "PUT",
        headers: { "Content-Type": "application/octet-stream", "X-QueryLab-CSRF": "1" },
        body: file.slice(offset, end),
        credentials: "same-origin",
        signal,
      });
    } catch (err) {
      if (err instanceof DOMException && err.name === "AbortError") throw err;
      if (++attempts > 3) throw new ApiError(0, "network_error", "The upload was interrupted.");
      continue;
    }
    const body = (await res.json().catch(() => undefined)) as
      | (Partial<JobView> & { error?: { code?: string; message?: string } })
      | undefined;
    if (res.status === 409 && typeof body?.error?.message === "string") {
      // The server reports where it is; continue from there.
      const match = body.error.message.match(/offset (\d+)/);
      if (match && ++attempts <= 3) {
        offset = Number(match[1]);
        continue;
      }
    }
    if (!res.ok) {
      throw new ApiError(res.status, body?.error?.code ?? "http_error", body?.error?.message ?? `Upload failed with status ${res.status}`);
    }
    attempts = 0;
    offset = end;
    onProgress(offset);
  }
}
