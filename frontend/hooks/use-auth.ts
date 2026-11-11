"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRouter } from "next/navigation";

import { api, ApiError } from "@/lib/api-client";
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

export function useLogin() {
  const qc = useQueryClient();
  return useMutation<User, ApiError, Credentials>({
    mutationFn: (body) => api.post<User>("/api/auth/login", body),
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
