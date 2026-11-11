"use client";

import { useQuery } from "@tanstack/react-query";

import { api, ApiError } from "@/lib/api-client";
import type { HealthResponse } from "@/types/api";

/**
 * Polls the backend health endpoint. A 503 still carries a JSON body
 * describing which dependency is degraded, so it is surfaced as data.
 */
export function useHealth() {
  return useQuery<HealthResponse, ApiError>({
    queryKey: ["health"],
    queryFn: async ({ signal }) => {
      try {
        return await api.get<HealthResponse>("/api/health", { signal });
      } catch (err) {
        if (err instanceof ApiError && err.status === 503) {
          return { status: "degraded", version: "unknown", database: "unavailable" };
        }
        throw err;
      }
    },
    refetchInterval: 30_000,
    retry: 1,
  });
}
