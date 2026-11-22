"use client";

import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { authKeys } from "@/hooks/use-auth";
import { api, ApiError } from "@/lib/api-client";
import type { AuditFilter, AuditPage, RecoveryCodes, SecurityStatus, SessionInfo, TotpEnrollment } from "@/types/account";

export const accountKeys = {
  security: ["account", "security"] as const,
  sessions: ["account", "sessions"] as const,
  options: ["account", "options"] as const,
  audit: (filter: AuditFilter) => ["audit", filter] as const,
};

/** Public: optional account features of this server. */
export function useAuthOptions() {
  return useQuery<{ password_reset: boolean }, ApiError>({
    queryKey: accountKeys.options,
    queryFn: ({ signal }) => api.get("/api/auth/options", { signal }),
    staleTime: 10 * 60_000,
  });
}

export function useSecurity() {
  return useQuery<SecurityStatus, ApiError>({
    queryKey: accountKeys.security,
    queryFn: ({ signal }) => api.get<SecurityStatus>("/api/auth/security", { signal }),
  });
}

export function useSessions() {
  return useQuery<SessionInfo[], ApiError>({
    queryKey: accountKeys.sessions,
    queryFn: ({ signal }) => api.get<SessionInfo[]>("/api/auth/sessions", { signal }),
  });
}

function useInvalidateAccount() {
  const qc = useQueryClient();
  return () => {
    void qc.invalidateQueries({ queryKey: ["account"] });
    void qc.invalidateQueries({ queryKey: authKeys.me });
    void qc.invalidateQueries({ queryKey: ["audit"] });
  };
}

export function useChangePassword() {
  const invalidate = useInvalidateAccount();
  return useMutation<{ other_sessions_revoked: number }, ApiError, { current_password: string; new_password: string }>({
    mutationFn: (body) => api.post("/api/auth/password", body),
    onSuccess: invalidate,
  });
}

export function useRevokeSession() {
  const invalidate = useInvalidateAccount();
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/auth/sessions/${id}`),
    onSuccess: invalidate,
  });
}

export function useRevokeOtherSessions() {
  const invalidate = useInvalidateAccount();
  return useMutation<{ revoked: number }, ApiError, void>({
    mutationFn: () => api.delete("/api/auth/sessions"),
    onSuccess: invalidate,
  });
}

export function useEnrollTotp() {
  return useMutation<TotpEnrollment, ApiError, void>({
    mutationFn: () => api.post<TotpEnrollment>("/api/auth/2fa/enroll"),
  });
}

export function useConfirmTotp() {
  const invalidate = useInvalidateAccount();
  return useMutation<RecoveryCodes, ApiError, string>({
    mutationFn: (code) => api.post<RecoveryCodes>("/api/auth/2fa/verify", { code }),
    onSuccess: invalidate,
  });
}

export function useRegenerateRecoveryCodes() {
  const invalidate = useInvalidateAccount();
  return useMutation<RecoveryCodes, ApiError, string>({
    mutationFn: (code) => api.post<RecoveryCodes>("/api/auth/2fa/recovery-codes", { code }),
    onSuccess: invalidate,
  });
}

export function useDisableTotp() {
  const invalidate = useInvalidateAccount();
  return useMutation<void, ApiError, { password: string; code: string }>({
    mutationFn: (body) => api.post<void>("/api/auth/2fa/disable", body),
    onSuccess: invalidate,
  });
}

export function useRequestReset() {
  return useMutation<void, ApiError, string>({
    mutationFn: (email) => api.post<void>("/api/auth/password-reset", { email }),
  });
}

export function useResetCheck(token: string | null) {
  return useQuery<{ valid: boolean }, ApiError>({
    queryKey: ["account", "reset", token],
    queryFn: ({ signal }) => api.get("/api/auth/password-reset", { signal, query: { token: token! } }),
    enabled: !!token,
    retry: false,
  });
}

export function useConfirmReset() {
  return useMutation<void, ApiError, { token: string; new_password: string }>({
    mutationFn: (body) => api.post<void>("/api/auth/password-reset/confirm", body),
  });
}

/** Audit events, newest first, loaded page by page. */
export function useAuditEvents(filter: AuditFilter) {
  return useInfiniteQuery<AuditPage, ApiError>({
    queryKey: accountKeys.audit(filter),
    initialPageParam: undefined as number | undefined,
    queryFn: ({ pageParam, signal }) =>
      api.get<AuditPage>("/api/audit-events", {
        signal,
        query: { ...filter, limit: 100, before: pageParam as number | undefined },
      }),
    getNextPageParam: (last) => last.next_before ?? undefined,
  });
}
