import type { NextConfig } from "next";

/**
 * The browser only ever talks to this Next.js origin. Requests to `/api/*`
 * are rewritten to the Rust API so the session cookie stays first-party
 * (HttpOnly, SameSite=Lax) and no cross-origin credentials are needed.
 */
const backendUrl = (process.env.BACKEND_URL ?? "http://127.0.0.1:8080").replace(/\/+$/, "");

const securityHeaders = [
  { key: "X-Content-Type-Options", value: "nosniff" },
  { key: "X-Frame-Options", value: "DENY" },
  { key: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
  { key: "Permissions-Policy", value: "camera=(), microphone=(), geolocation=()" },
];

const nextConfig: NextConfig = {
  poweredByHeader: false,
  turbopack: {
    rules: {
      "*.css": {
        loaders: ["@tailwindcss/turbopack"],
        as: "*.css",
      },
    },
  },
  async rewrites() {
    return [{ source: "/api/:path*", destination: `${backendUrl}/api/:path*` }];
  },
  async headers() {
    return [{ source: "/:path*", headers: securityHeaders }];
  },
};

export default nextConfig;
