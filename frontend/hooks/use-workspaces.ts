"use client";

import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { connectionKeys } from "@/hooks/use-connections";
import { api, ApiError } from "@/lib/api-client";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type {
  CreatedInvitation,
  GrantAccess,
  Invitation,
  InvitationPreview,
  Member,
  MemberAccess,
  Role,
  Snippet,
  SnippetInput,
  Workspace,
} from "@/types/workspaces";

export const workspaceKeys = {
  all: ["workspaces"] as const,
  members: (id: string) => ["workspaces", id, "members"] as const,
  invitations: (id: string) => ["workspaces", id, "invitations"] as const,
  grants: (connectionId: string) => ["connection-grants", connectionId] as const,
  snippets: (id: string | null) => ["snippets", id] as const,
};

export function useWorkspaces() {
  return useQuery<Workspace[], ApiError>({
    queryKey: workspaceKeys.all,
    queryFn: ({ signal }) => api.get<Workspace[]>("/api/workspaces", { signal }),
    staleTime: 60_000,
  });
}

/**
 * The workspace the app operates in: the stored choice when the user is
 * still a member, otherwise the personal workspace.
 */
export function useActiveWorkspace() {
  const { data: workspaces, isPending } = useWorkspaces();
  const activeId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const setActive = useWorkspaceStore((s) => s.setActiveWorkspace);
  const active =
    workspaces?.find((w) => w.id === activeId) ?? workspaces?.find((w) => w.personal) ?? workspaces?.[0] ?? null;

  useEffect(() => {
    if (!isPending && active && active.id !== activeId) setActive(active.id);
  }, [isPending, active, activeId, setActive]);

  return { workspace: active, workspaces: workspaces ?? [], isPending };
}

function useInvalidateWorkspace() {
  const qc = useQueryClient();
  return () => {
    void qc.invalidateQueries({ queryKey: workspaceKeys.all });
    void qc.invalidateQueries({ queryKey: connectionKeys.all });
  };
}

/**
 * Switches to a workspace the user just created or joined. It is added to
 * the cached list first: otherwise `useActiveWorkspace` would see the stale
 * list, not find it and fall back to the personal workspace before the
 * refetch lands.
 */
function useEnterWorkspace() {
  const qc = useQueryClient();
  const invalidate = useInvalidateWorkspace();
  const setActive = useWorkspaceStore((s) => s.setActiveWorkspace);
  return (ws: Workspace) => {
    qc.setQueryData<Workspace[]>(workspaceKeys.all, (old) =>
      old?.some((w) => w.id === ws.id) ? old : [...(old ?? []), ws],
    );
    setActive(ws.id);
    invalidate();
  };
}

export function useCreateWorkspace() {
  const enter = useEnterWorkspace();
  return useMutation<Workspace, ApiError, string>({
    mutationFn: (name) => api.post<Workspace>("/api/workspaces", { name }),
    onSuccess: enter,
  });
}

export function useRenameWorkspace(id: string) {
  const invalidate = useInvalidateWorkspace();
  return useMutation<Workspace, ApiError, string>({
    mutationFn: (name) => api.put<Workspace>(`/api/workspaces/${id}`, { name }),
    onSuccess: invalidate,
  });
}

export function useDeleteWorkspace() {
  const invalidate = useInvalidateWorkspace();
  const setActive = useWorkspaceStore((s) => s.setActiveWorkspace);
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/workspaces/${id}`),
    onSuccess: () => {
      setActive(null);
      invalidate();
    },
  });
}

export function useMembers(id: string | null) {
  return useQuery<Member[], ApiError>({
    queryKey: workspaceKeys.members(id ?? "none"),
    queryFn: ({ signal }) => api.get<Member[]>(`/api/workspaces/${id}/members`, { signal }),
    enabled: id !== null,
  });
}

export function useSetRole(id: string) {
  const qc = useQueryClient();
  return useMutation<Member[], ApiError, { userId: string; role: Role }>({
    mutationFn: ({ userId, role }) => api.put<Member[]>(`/api/workspaces/${id}/members/${userId}`, { role }),
    onSuccess: (members) => {
      qc.setQueryData(workspaceKeys.members(id), members);
      void qc.invalidateQueries({ queryKey: workspaceKeys.all });
    },
  });
}

export function useRemoveMember(id: string) {
  const qc = useQueryClient();
  const invalidate = useInvalidateWorkspace();
  return useMutation<void, ApiError, string>({
    mutationFn: (userId) => api.delete<void>(`/api/workspaces/${id}/members/${userId}`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: workspaceKeys.members(id) });
      invalidate();
    },
  });
}

export function useInvitations(id: string | null, enabled: boolean) {
  return useQuery<Invitation[], ApiError>({
    queryKey: workspaceKeys.invitations(id ?? "none"),
    queryFn: ({ signal }) => api.get<Invitation[]>(`/api/workspaces/${id}/invitations`, { signal }),
    enabled: id !== null && enabled,
  });
}

export function useInvite(id: string) {
  const qc = useQueryClient();
  return useMutation<CreatedInvitation, ApiError, { email: string; role: Role }>({
    mutationFn: (body) => api.post<CreatedInvitation>(`/api/workspaces/${id}/invitations`, body),
    onSuccess: () => void qc.invalidateQueries({ queryKey: workspaceKeys.invitations(id) }),
  });
}

export function useRevokeInvitation(id: string) {
  const qc = useQueryClient();
  return useMutation<void, ApiError, string>({
    mutationFn: (invitationId) => api.delete<void>(`/api/workspaces/${id}/invitations/${invitationId}`),
    onSuccess: () => void qc.invalidateQueries({ queryKey: workspaceKeys.invitations(id) }),
  });
}

export function useInvitationPreview(token: string | null) {
  return useQuery<InvitationPreview, ApiError>({
    queryKey: ["invitation", token],
    queryFn: ({ signal }) => api.get<InvitationPreview>(`/api/invitations/${encodeURIComponent(token!)}`, { signal }),
    enabled: !!token,
    retry: false,
  });
}

export function useAcceptInvitation() {
  const enter = useEnterWorkspace();
  return useMutation<Workspace, ApiError, string>({
    mutationFn: (token) => api.post<Workspace>(`/api/invitations/${encodeURIComponent(token)}/accept`),
    onSuccess: enter,
  });
}

export function useGrants(connectionId: string | null) {
  return useQuery<MemberAccess[], ApiError>({
    queryKey: workspaceKeys.grants(connectionId ?? "none"),
    queryFn: ({ signal }) => api.get<MemberAccess[]>(`/api/connections/${connectionId}/grants`, { signal }),
    enabled: connectionId !== null,
  });
}

export function useSetGrant(connectionId: string) {
  const qc = useQueryClient();
  return useMutation<MemberAccess[], ApiError, { userId: string; access: GrantAccess | null }>({
    mutationFn: ({ userId, access }) =>
      api.put<MemberAccess[]>(`/api/connections/${connectionId}/grants/${userId}`, { access }),
    onSuccess: (rows) => qc.setQueryData(workspaceKeys.grants(connectionId), rows),
  });
}

// -- snippets -----------------------------------------------------------------

export function useSnippets() {
  const workspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  return useQuery<Snippet[], ApiError>({
    queryKey: workspaceKeys.snippets(workspaceId),
    queryFn: ({ signal }) =>
      api.get<Snippet[]>("/api/snippets", { signal, query: { workspace_id: workspaceId ?? undefined } }),
  });
}

export function useSaveSnippet() {
  const qc = useQueryClient();
  return useMutation<Snippet, ApiError, { id?: string; body: SnippetInput }>({
    mutationFn: ({ id, body }) =>
      id
        ? api.put<Snippet>(`/api/snippets/${id}`, body)
        : api.post<Snippet>("/api/snippets", {
            workspace_id: useWorkspaceStore.getState().activeWorkspaceId ?? undefined,
            ...body,
          }),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["snippets"] }),
  });
}

export function useDeleteSnippet() {
  const qc = useQueryClient();
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/snippets/${id}`),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["snippets"] }),
  });
}
