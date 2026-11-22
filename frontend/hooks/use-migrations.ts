"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, ApiError } from "@/lib/api-client";
import type {
  DiffResponse,
  DiffSide,
  Migration,
  MigrationInput,
  MigrationProject,
  MigrationRun,
  RunOutcome,
  StatusResponse,
} from "@/types/migrations";

export const migrationKeys = {
  projects: ["migration-projects"] as const,
  migrations: (projectId: string) => ["migrations", projectId] as const,
  status: (projectId: string, connectionId: string) => ["migrations", projectId, "status", connectionId] as const,
  runs: (projectId: string) => ["migration-runs", projectId] as const,
};

export function useSchemaDiff() {
  return useMutation<DiffResponse, ApiError, { source: DiffSide; target: DiffSide }>({
    mutationFn: (body) => api.post<DiffResponse>("/api/schema-diff", body),
  });
}

export function useProjects() {
  return useQuery<MigrationProject[], ApiError>({
    queryKey: migrationKeys.projects,
    queryFn: ({ signal }) => api.get<MigrationProject[]>("/api/migration-projects", { signal }),
  });
}

export function useCreateProject() {
  const qc = useQueryClient();
  return useMutation<MigrationProject, ApiError, string>({
    mutationFn: (name) => api.post<MigrationProject>("/api/migration-projects", { name }),
    onSuccess: () => void qc.invalidateQueries({ queryKey: migrationKeys.projects }),
  });
}

export function useDeleteProject() {
  const qc = useQueryClient();
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/migration-projects/${id}`),
    onSuccess: () => void qc.invalidateQueries({ queryKey: migrationKeys.projects }),
  });
}

export function useMigrations(projectId: string | null) {
  return useQuery<Migration[], ApiError>({
    queryKey: migrationKeys.migrations(projectId ?? ""),
    queryFn: ({ signal }) => api.get<Migration[]>(`/api/migration-projects/${projectId}/migrations`, { signal }),
    enabled: projectId !== null,
  });
}

function useInvalidateProject() {
  const qc = useQueryClient();
  return (projectId: string) => {
    void qc.invalidateQueries({ queryKey: ["migrations", projectId] });
    void qc.invalidateQueries({ queryKey: migrationKeys.runs(projectId) });
    void qc.invalidateQueries({ queryKey: migrationKeys.projects });
  };
}

export function useCreateMigration() {
  const invalidate = useInvalidateProject();
  return useMutation<Migration, ApiError, { projectId: string; body: MigrationInput }>({
    mutationFn: ({ projectId, body }) => api.post<Migration>(`/api/migration-projects/${projectId}/migrations`, body),
    onSuccess: (m) => invalidate(m.project_id),
  });
}

export function useUpdateMigration() {
  const invalidate = useInvalidateProject();
  return useMutation<Migration, ApiError, { id: string; body: Partial<MigrationInput> }>({
    mutationFn: ({ id, body }) => api.put<Migration>(`/api/migrations/${id}`, body),
    onSuccess: (m) => invalidate(m.project_id),
  });
}

export function useDeleteMigration(projectId: string) {
  const invalidate = useInvalidateProject();
  return useMutation<void, ApiError, string>({
    mutationFn: (id) => api.delete<void>(`/api/migrations/${id}`),
    onSuccess: () => invalidate(projectId),
  });
}

export function useMigrationStatus(projectId: string | null, connectionId: string | null) {
  return useQuery<StatusResponse, ApiError>({
    queryKey: migrationKeys.status(projectId ?? "", connectionId ?? ""),
    queryFn: ({ signal }) =>
      api.get<StatusResponse>(`/api/migration-projects/${projectId}/status`, {
        signal,
        query: { connection_id: connectionId! },
      }),
    enabled: projectId !== null && connectionId !== null,
    retry: false,
  });
}

export function useRunMigration(projectId: string) {
  const invalidate = useInvalidateProject();
  return useMutation<
    RunOutcome,
    ApiError,
    { id: string; direction: "apply" | "rollback"; connection_id: string; confirm?: string; dry_run?: boolean }
  >({
    mutationFn: ({ id, direction, ...body }) => api.post<RunOutcome>(`/api/migrations/${id}/${direction}`, body),
    onSettled: () => invalidate(projectId),
  });
}

export function useMigrationRuns(projectId: string | null) {
  return useQuery<MigrationRun[], ApiError>({
    queryKey: migrationKeys.runs(projectId ?? ""),
    queryFn: ({ signal }) => api.get<MigrationRun[]>("/api/migration-runs", { signal, query: { project_id: projectId! } }),
    enabled: projectId !== null,
  });
}
