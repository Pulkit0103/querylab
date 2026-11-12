import type { ConnectionInput, SslMode } from "@/types/api";

const SSL_MODES: SslMode[] = ["disable", "prefer", "require", "verify-ca", "verify-full"];

/**
 * Parses a `postgres://user:password@host:port/database?sslmode=...` URL into
 * form fields. Returns null when the input is not a PostgreSQL URL.
 * Parsing happens entirely in the browser; the URL is never sent as-is.
 */
export function parseConnectionString(input: string): Partial<ConnectionInput> | null {
  const trimmed = input.trim();
  if (!/^postgres(ql)?:\/\//i.test(trimmed)) return null;
  let url: URL;
  try {
    // URL only understands registered schemes for host parsing; swap to http.
    url = new URL(trimmed.replace(/^postgres(ql)?:\/\//i, "http://"));
  } catch {
    return null;
  }
  const out: Partial<ConnectionInput> = {};
  if (url.hostname) out.host = url.hostname.replace(/^\[|\]$/g, "");
  out.port = url.port ? Number(url.port) : 5432;
  const db = decodeURIComponent(url.pathname.replace(/^\//, ""));
  if (db) out.database = db;
  if (url.username) out.username = decodeURIComponent(url.username);
  if (url.password) out.password = decodeURIComponent(url.password);
  const sslmode = url.searchParams.get("sslmode") as SslMode | null;
  if (sslmode && SSL_MODES.includes(sslmode)) out.ssl_mode = sslmode;
  if (out.host && !out.ssl_mode && !/^(localhost|127\.|\[?::1)/.test(out.host)) out.ssl_mode = "require";
  return out;
}
