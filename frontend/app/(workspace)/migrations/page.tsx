"use client";

import { Suspense, useState } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { FolderGit2, MoreHorizontal, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { EmptyState } from "@/components/common/empty-state";
import { MigrationEditor } from "@/components/migrations/migration-editor";
import { RunDialog, type RunDirection } from "@/components/migrations/run-dialog";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useActiveConnection } from "@/hooks/use-connections";
import {
  useCreateProject,
  useDeleteMigration,
  useDeleteProject,
  useMigrationRuns,
  useMigrations,
  useMigrationStatus,
  useProjects,
} from "@/hooks/use-migrations";
import { formatDuration, formatRelativeTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { Migration, MigrationState } from "@/types/migrations";

const STATE_CLASS: Record<MigrationState, string> = {
  applied: "bg-success/15 text-success",
  pending: "bg-muted text-muted-foreground",
  drift: "bg-destructive/15 text-destructive",
};

function NewProject({ onCreated }: { onCreated: (id: string) => void }) {
  const [name, setName] = useState("");
  const create = useCreateProject();
  return (
    <form
      className="flex gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        create.mutate(name.trim(), {
          onSuccess: (p) => {
            setName("");
            onCreated(p.id);
          },
          onError: (err) => toast.error(err.message),
        });
      }}
    >
      <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="New project" aria-label="New project name" className="h-8 w-48 text-xs" maxLength={100} />
      <Button size="sm" variant="outline" className="h-8" disabled={!name.trim() || create.isPending}>
        <Plus className="size-3.5" /> Create
      </Button>
    </form>
  );
}

function MigrationsView() {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const { connection, connections, isPending } = useActiveConnection();
  const projects = useProjects();
  const deleteProject = useDeleteProject();
  const projectId = params.get("project") ?? projects.data?.[0]?.id ?? null;
  const [connectionId, setConnectionId] = useState<string | null>(null);
  const target = connections.find((c) => c.id === (connectionId ?? connection?.id)) ?? null;
  const migrations = useMigrations(projectId);
  const status = useMigrationStatus(projectId, target?.id ?? null);
  const runs = useMigrationRuns(projectId);
  const remove = useDeleteMigration(projectId ?? "");
  const [editing, setEditing] = useState<{ migration: Migration | null } | null>(null);
  const [running, setRunning] = useState<{ migration: Migration; direction: RunDirection } | null>(null);

  if (isPending || projects.isPending) return <Skeleton className="m-4 h-64" />;
  const select = (id: string) => router.replace(`${pathname}?project=${id}`);
  const project = projects.data?.find((p) => p.id === projectId) ?? null;
  const stateOf = (id: string) => status.data?.migrations.find((m) => m.id === id);
  const connectionItems = connections.map((c) => ({ value: c.id, label: `${c.name} (${c.environment})` }));

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-3 border-b bg-panel px-3 py-2">
        <FolderGit2 className="size-4 text-brand" aria-hidden />
        {(projects.data?.length ?? 0) > 0 && (
          <Select
            items={(projects.data ?? []).map((p) => ({ value: p.id, label: p.name }))}
            value={projectId}
            onValueChange={(v) => typeof v === "string" && select(v)}
          >
            <SelectTrigger size="sm" className="h-8 w-52 text-xs" aria-label="Project">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {(projects.data ?? []).map((p) => (
                <SelectItem key={p.id} value={p.id} className="text-xs">
                  {p.name} <span className="ml-auto text-muted-foreground">{p.migrations}</span>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
        <NewProject onCreated={select} />
        {project && (
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Delete project ${project.name}`}
            onClick={() => {
              if (window.confirm(`Delete project “${project.name}” and its ${project.migrations} scripts? Databases keep their applied state.`))
                deleteProject.mutate(project.id, { onSuccess: () => router.replace(pathname), onError: (e) => toast.error(e.message) });
            }}
          >
            <Trash2 className="size-3.5" />
          </Button>
        )}
        <div className="ml-auto flex items-center gap-2 text-xs">
          <span className="text-muted-foreground">Status on</span>
          <Select items={connectionItems} value={target?.id ?? null} onValueChange={(v) => typeof v === "string" && setConnectionId(v)}>
            <SelectTrigger size="sm" className="h-8 w-56 text-xs" aria-label="Target connection">
              <SelectValue placeholder="Connection" />
            </SelectTrigger>
            <SelectContent>
              {connectionItems.map((c) => (
                <SelectItem key={c.value} value={c.value} className="text-xs">
                  {c.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {project && (
            <Button size="sm" className="h-8 gap-1" onClick={() => setEditing({ migration: null })}>
              <Plus className="size-3.5" /> New migration
            </Button>
          )}
        </div>
      </div>

      {!project ? (
        <EmptyState
          icon={FolderGit2}
          title="No migration projects"
          description="Create a project (for example one per application), then write migrations by hand or save them from Schema Diff."
        />
      ) : (
        <div className="min-h-0 flex-1 space-y-5 overflow-auto p-3 scrollbar-thin">
          {status.isError && <p className="text-sm text-destructive">Status unavailable: {status.error.message}</p>}
          {status.data && !status.data.tracking_table && (
            <p className="text-xs text-muted-foreground">
              No migrations have been applied to {target?.name} yet. The first apply creates <span className="font-mono">querylab.schema_migrations</span> there.
            </p>
          )}
          {status.data?.note && <p className="text-xs text-warning">{status.data.note}</p>}
          {status.data?.migrations.some((m) => m.state === "drift") && (
            <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs" role="alert">
              A script was changed after it was applied to this database (checksum mismatch). Further applies are blocked until the
              applied script is restored.
            </p>
          )}

          <section aria-label="Migrations">
            {migrations.data?.length === 0 ? (
              <p className="py-8 text-center text-sm text-muted-foreground">No migrations in this project yet.</p>
            ) : (
              <div className="overflow-x-auto rounded-lg border">
                <table className="w-full text-left text-xs">
                  <thead className="bg-panel text-muted-foreground">
                    <tr>
                      <th className="px-3 py-2 font-medium">Version</th>
                      <th className="w-full px-3 py-2 font-medium">Name</th>
                      <th className="px-3 py-2 font-medium whitespace-nowrap">Status</th>
                      <th className="px-3 py-2 font-medium">Applied</th>
                      <th className="px-3 py-2">
                        <span className="sr-only">Actions</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {(migrations.data ?? []).map((m) => {
                      const s = stateOf(m.id);
                      return (
                        <tr key={m.id} className="border-t align-top hover:bg-accent/30" data-version={m.version}>
                          <td className="px-3 py-2 font-mono tabular-nums">{m.version}</td>
                          <td className="px-3 py-2">
                            <span className="font-medium">{m.name}</span>
                            <span className="block font-mono text-[11px] text-muted-foreground">
                              {m.up_sql.split("\n").find((l) => l.trim() && !l.trim().startsWith("--") && !l.startsWith("SET LOCAL"))?.slice(0, 120)}
                            </span>
                          </td>
                          <td className="px-3 py-2">
                            {s ? (
                              <span className={cn("rounded px-1.5 py-0.5 text-[10px] font-medium", STATE_CLASS[s.state])}>{s.state}</span>
                            ) : (
                              <span className="text-muted-foreground">—</span>
                            )}
                          </td>
                          <td className="px-3 py-2 whitespace-nowrap text-muted-foreground">
                            {s?.applied_at ? (
                              <span title={`${s.applied_by} · ${formatDuration(s.duration_ms)}`}>{formatRelativeTime(s.applied_at)}</span>
                            ) : (
                              "—"
                            )}
                          </td>
                          <td className="px-2 py-1.5">
                            <div className="flex items-center gap-1">
                              {s?.state === "pending" && (
                                <Button size="xs" onClick={() => setRunning({ migration: m, direction: "apply" })}>
                                  Apply
                                </Button>
                              )}
                              <DropdownMenu>
                                <DropdownMenuTrigger render={<Button variant="ghost" size="icon-xs" aria-label={`Actions for migration ${m.version}`} />}>
                                  <MoreHorizontal className="size-3.5" />
                                </DropdownMenuTrigger>
                                <DropdownMenuContent align="end" className="w-48">
                                  <DropdownMenuItem disabled={m.locked} onClick={() => setEditing({ migration: m })}>
                                    {m.locked ? "Applied: read-only" : "Edit"}
                                  </DropdownMenuItem>
                                  <DropdownMenuItem
                                    disabled={s?.state !== "applied" || !m.down_sql}
                                    onClick={() => setRunning({ migration: m, direction: "rollback" })}
                                  >
                                    Roll back{!m.down_sql && " (no down script)"}
                                  </DropdownMenuItem>
                                  <DropdownMenuSeparator />
                                  <DropdownMenuItem
                                    variant="destructive"
                                    disabled={m.locked}
                                    onClick={() =>
                                      window.confirm(`Delete migration ${m.version}?`) &&
                                      remove.mutate(m.id, { onError: (e) => toast.error(e.message) })
                                    }
                                  >
                                    Delete
                                  </DropdownMenuItem>
                                </DropdownMenuContent>
                              </DropdownMenu>
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
            {(status.data?.unknown.length ?? 0) > 0 && (
              <p className="mt-2 text-xs text-muted-foreground">
                Also recorded in this database without a script here: {status.data!.unknown.map((u) => `${u.version} (${u.name})`).join(", ")}.
              </p>
            )}
          </section>

          <section aria-label="Run history">
            <h3 className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">History</h3>
            {(runs.data?.length ?? 0) === 0 ? (
              <p className="text-xs text-muted-foreground">Nothing applied yet.</p>
            ) : (
              <ol className="space-y-1.5 border-l pl-4 text-xs">
                {runs.data!.map((r) => (
                  <li key={r.id} className="relative">
                    <span
                      className={cn(
                        "absolute top-1 -left-[21px] size-2.5 rounded-full border-2 border-background",
                        r.status === "success" ? "bg-success" : "bg-destructive",
                      )}
                    />
                    <span className="font-medium">
                      {r.direction === "up" ? "Applied" : "Rolled back"} {r.version}
                    </span>{" "}
                    <span className="text-muted-foreground">
                      {r.name} on {r.connection_name ?? "deleted connection"} · {formatDuration(r.duration_ms)} · {formatRelativeTime(r.started_at)}
                    </span>
                    {r.status === "error" && <span className="block font-mono text-destructive">{r.error_message}</span>}
                  </li>
                ))}
              </ol>
            )}
          </section>
        </div>
      )}

      {project && (
        <MigrationEditor
          projectId={project.id}
          migration={editing?.migration ?? null}
          open={editing !== null}
          onOpenChange={(open) => !open && setEditing(null)}
        />
      )}
      {running && target && (
        <RunDialog
          key={`${running.migration.id}:${running.direction}:${target.id}`}
          migration={running.migration}
          direction={running.direction}
          connection={target}
          onClose={() => setRunning(null)}
        />
      )}
    </div>
  );
}

export default function MigrationsPage() {
  return (
    <Suspense fallback={<Skeleton className="m-4 h-64" />}>
      <MigrationsView />
    </Suspense>
  );
}
