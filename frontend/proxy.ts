import { NextResponse, type NextRequest } from "next/server";

import { SESSION_COOKIES } from "@/lib/auth";

/**
 * Optimistic route guard: sends visitors without a session cookie to the
 * login page. The API remains the authority — an invalid or expired cookie
 * is rejected there and the client redirects to login on 401.
 */
export function proxy(request: NextRequest) {
  const hasSession = SESSION_COOKIES.some((name) => request.cookies.has(name));
  if (hasSession) return NextResponse.next();

  const url = request.nextUrl.clone();
  const next = `${request.nextUrl.pathname}${request.nextUrl.search}`;
  url.pathname = "/login";
  url.search = next && next !== "/" ? `?next=${encodeURIComponent(next)}` : "";
  return NextResponse.redirect(url);
}

export const config = {
  // Everything except the API proxy, auth pages, Next internals and static files.
  matcher: ["/((?!api|login|register|share|_next/static|_next/image|favicon.ico|.*\\.(?:png|svg|ico|webp)$).*)"],
};
