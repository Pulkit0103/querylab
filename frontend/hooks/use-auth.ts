"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRouter } from "next/navigation";

import { api, ApiError } from "@/lib/api-client";
import type { TwoFactorChallenge } from "@/types/account";
import type { User } from "@/types/api";

export const authKeys = { me: ["auth", "me"] as const };

export function useMe() {
  return useQuery<User, ApiError>({
    queryKey: authKeys.me,
    queryFn: ({ signal }) => api.get<User>("/api/auth/me", { signal }),
    staleTime: 5 * 60_000,
    retry: false,
  });
}

interface Credentials {
  email: string;
  password: string;
  display_name?: string;
}

export const needsSecondFactor = (r: User | TwoFactorChallenge): r is TwoFactorChallenge =>
  (r as TwoFactorChallenge).two_factor_required === true;

/** Password step. Accounts with 2FA get a challenge instead of a session. */
export function useLogin() {
  const qc = useQueryClient();
  return useMutation<User | TwoFactorChallenge, ApiError, Credentials>({
    mutationFn: (body) => api.post<User | TwoFactorChallenge>("/api/auth/login", body),
    onSuccess: (result) => {
      if (needsSecondFactor(result)) return;
      qc.clear();
      qc.setQueryData(authKeys.me, result);
    },
  });
}

/** Second step: a TOTP or recovery code for the challenge. */
export function useLoginSecondFactor() {
  const qc = useQueryClient();
  return useMutation<User, ApiError, { challenge: string; code: string }>({
    mutationFn: (body) => api.post<User>("/api/auth/login/2fa", body),
    onSuccess: (user) => {
      qc.clear();
      qc.setQueryData(authKeys.me, user);
    },
  });
}

export function useRegister() {
  const qc = useQueryClient();
  return useMutation<User, ApiError, Credentials>({
    mutationFn: (body) => api.post<User>("/api/auth/register", body),
    onSuccess: (user) => {
      qc.clear();
      qc.setQueryData(authKeys.me, user);
    },
  });
}

export function useLogout() {
  const qc = useQueryClient();
  const router = useRouter();
  return useMutation<void, ApiError, void>({
    mutationFn: () => api.post<void>("/api/auth/logout"),
    onSettled: () => {
      // Drop every cached response belonging to the previous user.
      qc.clear();
      router.replace("/login");
      router.refresh();
    },
  });
}
