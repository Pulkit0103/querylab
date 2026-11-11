/** Session cookie names set by the API (secure deployments use `__Host-`). */
export const SESSION_COOKIES = ["__Host-ql_session", "ql_session"] as const;

export const PUBLIC_PATHS = ["/login", "/register"] as const;

/**
 * Returns a safe in-app redirect target. Only same-origin absolute paths are
 * allowed, which prevents open redirects via `?next=`.
 */
export function safeNextPath(next: string | null | undefined, fallback = "/editor"): string {
  if (!next || !next.startsWith("/") || next.startsWith("//") || next.startsWith("/\\")) return fallback;
  if (PUBLIC_PATHS.some((p) => next === p || next.startsWith(`${p}?`))) return fallback;
  return next;
}

export function loginUrl(currentPath: string): string {
  const next = safeNextPath(currentPath, "");
  return next ? `/login?next=${encodeURIComponent(next)}` : "/login";
}
