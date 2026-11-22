/** Server-generated downloads (exports) saved as files in the browser. */
import { ApiError, errorMessageFor } from "@/lib/api-client";
import type { ExportFormat } from "@/types/data";

export const EXPORT_FORMATS: { format: ExportFormat; label: string }[] = [
  { format: "csv", label: "CSV" },
  { format: "json", label: "JSON" },
  { format: "xlsx", label: "Excel (.xlsx)" },
  { format: "sql", label: "SQL INSERT statements" },
];

/** Rows each format stops at (mirrors the API). */
export const EXPORT_ROW_LIMIT: Record<ExportFormat, number> = {
  csv: 1_000_000,
  json: 1_000_000,
  sql: 1_000_000,
  xlsx: 200_000,
};

export function filenameFrom(disposition: string | null, fallback: string): string {
  const match = disposition?.match(/filename="([^"]+)"/);
  return match?.[1] ?? fallback;
}

export function saveBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/**
 * POSTs `body` to `path` and saves the streamed response. Errors before
 * the download starts carry the API's message; a failure after streaming
 * started aborts the body, so this rejects instead of saving a partial file.
 */
export async function postDownload(path: string, body: unknown, fallbackName: string): Promise<void> {
  let res: Response;
  try {
    res = await fetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-QueryLab-CSRF": "1" },
      body: JSON.stringify(body),
      credentials: "same-origin",
      cache: "no-store",
    });
  } catch {
    throw new ApiError(0, "network_error", "Unable to reach the QueryLab API.");
  }
  if (!res.ok) {
    const data = (await res.json().catch(() => undefined)) as
      | { error?: { code?: string; message?: string; details?: unknown } }
      | undefined;
    const requestId = res.headers.get("x-request-id");
    throw new ApiError(
      res.status,
      data?.error?.code ?? "http_error",
      errorMessageFor(res.status, data?.error?.message ?? `Export failed with status ${res.status}`, requestId, data?.error?.code),
      data?.error?.details,
      requestId ?? undefined,
    );
  }
  const blob = await res.blob();
  saveBlob(blob, filenameFrom(res.headers.get("content-disposition"), fallbackName));
}

/** The full result of one read-only statement, streamed by the server. */
export function downloadQueryExport(
  connectionId: string,
  sql: string,
  format: ExportFormat,
  name?: string,
): Promise<void> {
  return postDownload(
    `/api/connections/${connectionId}/exports`,
    { format, source: { type: "query", sql, name } },
    `${name ?? "query_result"}.${format}`,
  );
}
