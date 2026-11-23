"use client";

import { useMemo, useState, type DragEvent } from "react";
import {
  ChevronRight,
  Copy,
  Database,
  FolderInput,
  FolderPlus,
  Loader2,
  MoreHorizontal,
  Pencil,
  RefreshCw,
  Search,
  ShieldCheck,
  Trash2,
  UserCog,
} from "lucide-react";
import { toast } from "sonner";

import { EmptyState } from "@/components/common/empty-state";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { ConnectionAccessDialog } from "@/components/workspaces/connection-access-dialog";
import { useActiveWorkspace } from "@/hooks/use-workspaces";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  useConnectionGroups,
  useConnections,
  useCreateGroup,
  useDeleteConnection,
  useDeleteGroup,
  useDuplicateConnection,
  useReconnect,
  useUpdateConnection,
  useUpdateGroup,
} from "@/hooks/use-connections";
import {
  ENVIRONMENT_BADGE,
  ENVIRONMENTS,
  environmentLabel,
  groupConnections,
  matchesConnection,
  STATUS_LABEL,
  statusTone,
} from "@/lib/connection-meta";
import { formatDuration, formatRelativeTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { Connection, ConnectionGroup, Environment } from "@/types/api";

const DRAG_TYPE = "application/x-querylab-connection";

const TONE_CLASS = {
  success: "bg-success",
  warning: "bg-warning",
  destructive: "bg-destructive",
  muted: "bg-muted-foreground/40",
} as const;

export function StatusDot({ connection, className }: { connection: Connection; className?: string }) {
  const status = connection.last_status;
  const when = connection.last_status_at ? ` · ${formatRelativeTime(connection.last_status_at)}` : "";
  const label =
    status === null
      ? "Not tested yet"
      : status === "online"
        ? `Online · ${formatDuration(connection.last_latency_ms)}${when}`
        : `${STATUS_LABEL[status]}${when}${connection.last_error ? ` — ${connection.last_error}` : ""}`;
  return (
    <Tooltip>
      <TooltipTrigger render={<span className={cn("flex size-4 shrink-0 items-center justify-center", className)} />}>
        <span role="img" aria-label={status ? STATUS_LABEL[status] : "Not tested"} className={cn("size-2 rounded-full", TONE_CLASS[statusTone(status)])} />
      </TooltipTrigger>
      <TooltipContent className="max-w-xs">{label}</TooltipContent>
    </Tooltip>
  );
}

export function EnvironmentBadge({ environment }: { environment: Environment }) {
  if (environment === "development") return null;
  return (
    <Badge variant="outline" className={cn("text-[10px] uppercase", ENVIRONMENT_BADGE[environment])}>
      {environmentLabel(environment)}
    </Badge>
  );
}

function ConnectionRow({
  connection,
  groups,
  onDelete,
}: {
  connection: Connection;
  groups: ConnectionGroup[];
  onDelete: () => void;
}) {
  const openDialog = useWorkspaceStore((s) => s.openConnectionDialog);
  const reconnect = useReconnect();
  const duplicate = useDuplicateConnection();
  const update = useUpdateConnection();
  const { workspace } = useActiveWorkspace();
  const [sharing, setSharing] = useState(false);
  const manage = connection.access === "admin";

  const moveTo = (groupId: string | null) =>
    update.mutate(
      { id: connection.id, body: { group_id: groupId } },
      { onError: (e) => toast.error(e.message) },
    );

  return (
    <li
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData(DRAG_TYPE, connection.id);
        e.dataTransfer.effectAllowed = "move";
      }}
      className="relative flex items-center gap-3 py-2.5 pr-2 pl-4"
    >
      {connection.color && (
        <span aria-hidden className="absolute inset-y-2 left-1 w-1 rounded-full" style={{ background: connection.color }} />
      )}
      <StatusDot connection={connection} />
      <div className="min-w-0 flex-1">
        <p className="flex items-center gap-2 truncate text-sm font-medium">
          <span className="truncate">{connection.name}</span>
          <EnvironmentBadge environment={connection.environment} />
          {!manage && (
            <Badge variant="outline" className="text-[10px] text-muted-foreground">
              {connection.access === "write" ? "read & write access" : "read access"}
            </Badge>
          )}
          {connection.read_only ? (
            <Badge variant="outline" className="gap-1 border-success/30 text-[10px] text-success">
              <ShieldCheck className="size-3" /> read-only
            </Badge>
          ) : (
            <Badge variant="outline" className="border-warning/30 text-[10px] text-warning">
              read-write
            </Badge>
          )}
        </p>
        <p className="truncate font-mono text-xs text-muted-foreground">
          {connection.username}@{connection.host}:{connection.port}/{connection.database} · ssl {connection.ssl_mode}
          {connection.ssl_root_cert ? " + CA" : ""}
        </p>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <Button
          size="sm"
          variant="ghost"
          className="gap-1.5"
          disabled={reconnect.isPending}
          onClick={() =>
            reconnect.mutate(connection.id, {
              onSuccess: (r) => toast.success(`${connection.name}: online in ${formatDuration(r.latency_ms)}`),
              onError: (e) => toast.error(`${connection.name}: ${e.message}`),
            })
          }
        >
          {reconnect.isPending ? <Loader2 className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}
          <span className="hidden sm:inline">{connection.last_status && connection.last_status !== "online" ? "Reconnect" : "Test"}</span>
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={<Button size="icon-sm" variant="ghost" aria-label={`Actions for ${connection.name}`} />}
          >
            <MoreHorizontal className="size-3.5" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-48">
            {manage && (
            <>
            <DropdownMenuItem onClick={() => openDialog(connection)}>
              <Pencil /> Edit
            </DropdownMenuItem>
            {workspace && !workspace.personal && (
              <DropdownMenuItem onClick={() => setSharing(true)}>
                <UserCog /> Access
              </DropdownMenuItem>
            )}
            <DropdownMenuItem
              onClick={() =>
                duplicate.mutate(connection.id, {
                  onSuccess: (c) => toast.success(`Created ${c.name}`),
                  onError: (e) => toast.error(e.message),
                })
              }
            >
              <Copy /> Duplicate
            </DropdownMenuItem>
            <DropdownMenuSub>
              <DropdownMenuSubTrigger>
                <FolderInput /> Move to group
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent className="w-44">
                <DropdownMenuItem disabled={connection.group_id === null} onClick={() => moveTo(null)}>
                  Ungrouped
                </DropdownMenuItem>
                {groups.map((g) => (
                  <DropdownMenuItem key={g.id} disabled={connection.group_id === g.id} onClick={() => moveTo(g.id)}>
                    <span className="truncate">{g.name}</span>
                  </DropdownMenuItem>
                ))}
              </DropdownMenuSubContent>
            </DropdownMenuSub>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onClick={onDelete}>
              <Trash2 /> Delete
            </DropdownMenuItem>
            </>
            )}
            {!manage && <DropdownMenuItem disabled>Only connection admins can change it</DropdownMenuItem>}
          </DropdownMenuContent>
        </DropdownMenu>
        <ConnectionAccessDialog connection={sharing ? connection : null} onClose={() => setSharing(false)} />
      </div>
    </li>
  );
}

function GroupNameDialog({
  open,
  title,
  initial,
  pending,
  onSubmit,
  onOpenChange,
}: {
  open: boolean;
  title: string;
  initial: string;
  pending: boolean;
  onSubmit: (name: string) => void;
  onOpenChange: (open: boolean) => void;
}) {
  const [name, setName] = useState(initial);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (name.trim()) onSubmit(name.trim());
          }}
          className="space-y-4"
        >
          <Input
            autoFocus
            aria-label="Group name"
            value={name}
            maxLength={100}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. Analytics"
          />
          <DialogFooter>
            <Button type="submit" disabled={!name.trim() || pending}>
              {pending && <Loader2 className="size-3.5 animate-spin" />} Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function SectionHeader({
  group,
  count,
  collapsed,
  onToggle,
  onRename,
  onDelete,
}: {
  group: ConnectionGroup | null;
  count: number;
  collapsed: boolean;
  onToggle: () => void;
  onRename: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="flex items-center gap-1 bg-muted/30 px-2 py-1">
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={!collapsed}
        className="flex min-w-0 flex-1 items-center gap-1.5 rounded px-1 py-0.5 text-left text-xs font-medium text-muted-foreground hover:text-foreground"
      >
        <ChevronRight className={cn("size-3.5 transition-transform", !collapsed && "rotate-90")} />
        <span className="truncate">{group ? group.name : "Ungrouped"}</span>
        <span className="tabular-nums text-muted-foreground/70">{count}</span>
      </button>
      {group && (
        <DropdownMenu>
          <DropdownMenuTrigger render={<Button size="icon-xs" variant="ghost" aria-label={`Group ${group.name} actions`} />}>
            <MoreHorizontal className="size-3" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-40">
            <DropdownMenuItem onClick={onRename}>
              <Pencil /> Rename
            </DropdownMenuItem>
            <DropdownMenuItem variant="destructive" onClick={onDelete}>
              <Trash2 /> Delete group
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}

export function ConnectionList() {
  const { data, isPending, isError, error } = useConnections();
  const { data: groups = [] } = useConnectionGroups();
  const openDialog = useWorkspaceStore((s) => s.openConnectionDialog);
  const del = useDeleteConnection();
  const update = useUpdateConnection();
  const createGroup = useCreateGroup();
  const updateGroup = useUpdateGroup();
  const deleteGroup = useDeleteGroup();

  const [query, setQuery] = useState("");
  const [env, setEnv] = useState<Environment | "all">("all");
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<Connection | null>(null);
  const [pendingGroupDelete, setPendingGroupDelete] = useState<ConnectionGroup | null>(null);
  const [groupDialog, setGroupDialog] = useState<{ mode: "create" } | { mode: "rename"; group: ConnectionGroup } | null>(null);

  const filtered = useMemo(
    () => (data ?? []).filter((c) => (env === "all" || c.environment === env) && matchesConnection(c, query)),
    [data, env, query],
  );
  const filtering = query.trim() !== "" || env !== "all";
  const sections = useMemo(() => {
    const all = groupConnections(filtered, groups);
    // While filtering, hide sections with no matches.
    return filtering ? all.filter((s) => s.connections.length > 0) : all;
  }, [filtered, groups, filtering]);

  if (isPending) {
    return (
      <div className="space-y-2 p-4">
        <Skeleton className="h-10" />
        <Skeleton className="h-10" />
      </div>
    );
  }
  if (isError) return <p className="p-4 text-sm text-destructive">{error.message}</p>;
  if (data.length === 0) {
    return (
      <EmptyState
        className="py-10"
        icon={Database}
        title="No connections yet"
        description="Add a PostgreSQL database to start querying."
        action={
          <Button size="sm" onClick={() => openDialog()} className="gap-1.5">
            <Database className="size-3.5" /> Add connection
          </Button>
        }
      />
    );
  }

  const key = (g: ConnectionGroup | null) => g?.id ?? "__ungrouped";
  const onDragOver = (e: DragEvent, target: string) => {
    if (!e.dataTransfer.types.includes(DRAG_TYPE)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    setDropTarget(target);
  };
  const onDrop = (e: DragEvent, group: ConnectionGroup | null) => {
    e.preventDefault();
    setDropTarget(null);
    const id = e.dataTransfer.getData(DRAG_TYPE);
    const conn = data.find((c) => c.id === id);
    if (!conn || conn.group_id === (group?.id ?? null)) return;
    update.mutate(
      { id, body: { group_id: group?.id ?? null } },
      {
        onSuccess: () => toast.success(`Moved ${conn.name} to ${group?.name ?? "Ungrouped"}`),
        onError: (err) => toast.error(err.message),
      },
    );
  };

  return (
    <>
      <div className="flex flex-wrap items-center gap-2 border-b px-4 py-2">
        <div className="relative min-w-40 flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search connections"
            aria-label="Search connections"
            className="h-8 pl-8 text-xs"
          />
        </div>
        <Select
          items={[{ value: "all", label: "All environments" }, ...ENVIRONMENTS.map((e) => ({ value: e.value, label: e.label }))]}
          value={env}
          onValueChange={(v) => v && setEnv(v as Environment | "all")}
        >
          <SelectTrigger className="h-8 w-40 text-xs" aria-label="Filter by environment">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All environments</SelectItem>
            {ENVIRONMENTS.map((e) => (
              <SelectItem key={e.value} value={e.value}>
                {e.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button size="sm" variant="ghost" className="h-8 gap-1.5 text-xs" onClick={() => setGroupDialog({ mode: "create" })}>
          <FolderPlus className="size-3.5" /> New group
        </Button>
      </div>

      {sections.length === 0 ? (
        <p className="px-4 py-6 text-center text-xs text-muted-foreground">No connections match the current filters.</p>
      ) : (
        <div className="divide-y">
          {sections.map(({ group, connections }) => {
            const k = key(group);
            const isCollapsed = collapsed.has(k);
            return (
              <section
                key={k}
                aria-label={group ? `Group ${group.name}` : "Ungrouped connections"}
                onDragOver={(e) => onDragOver(e, k)}
                onDragLeave={() => setDropTarget((t) => (t === k ? null : t))}
                onDrop={(e) => onDrop(e, group)}
                className={cn(dropTarget === k && "bg-brand/5 outline outline-1 -outline-offset-1 outline-brand/40")}
              >
                {(groups.length > 0 || group) && (
                  <SectionHeader
                    group={group}
                    count={connections.length}
                    collapsed={isCollapsed}
                    onToggle={() =>
                      setCollapsed((s) => {
                        const next = new Set(s);
                        if (next.has(k)) next.delete(k);
                        else next.add(k);
                        return next;
                      })
                    }
                    onRename={() => group && setGroupDialog({ mode: "rename", group })}
                    onDelete={() => group && setPendingGroupDelete(group)}
                  />
                )}
                {!isCollapsed &&
                  (connections.length > 0 ? (
                    <ul className="divide-y">
                      {connections.map((c) => (
                        <ConnectionRow key={c.id} connection={c} groups={groups} onDelete={() => setPendingDelete(c)} />
                      ))}
                    </ul>
                  ) : (
                    <p className="px-4 py-3 text-xs text-muted-foreground">Drag connections here.</p>
                  ))}
              </section>
            );
          })}
        </div>
      )}

      {groupDialog && (
        <GroupNameDialog
          key={groupDialog.mode === "rename" ? groupDialog.group.id : "new"}
          open
          title={groupDialog.mode === "create" ? "New connection group" : "Rename group"}
          initial={groupDialog.mode === "rename" ? groupDialog.group.name : ""}
          pending={createGroup.isPending || updateGroup.isPending}
          onOpenChange={(o) => !o && setGroupDialog(null)}
          onSubmit={(name) => {
            const done = { onSuccess: () => setGroupDialog(null), onError: (e: Error) => toast.error(e.message) };
            if (groupDialog.mode === "create") createGroup.mutate({ name }, done);
            else updateGroup.mutate({ id: groupDialog.group.id, name }, done);
          }}
        />
      )}

      <AlertDialog open={pendingGroupDelete !== null} onOpenChange={(o) => !o && setPendingGroupDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete group “{pendingGroupDelete?.name}”?</AlertDialogTitle>
            <AlertDialogDescription>The group is removed. Its connections are kept and become ungrouped.</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                if (!pendingGroupDelete) return;
                deleteGroup.mutate(pendingGroupDelete.id, { onError: (e) => toast.error(e.message) });
                setPendingGroupDelete(null);
              }}
            >
              Delete group
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={pendingDelete !== null} onOpenChange={(o) => !o && setPendingDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete “{pendingDelete?.name}”?</AlertDialogTitle>
            <AlertDialogDescription>
              The stored credentials are removed permanently. Saved queries and history are kept but no longer linked
              to this connection.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                if (!pendingDelete) return;
                const target = pendingDelete;
                del.mutate(target.id, {
                  onSuccess: () => toast.success(`Deleted ${target.name}`),
                  onError: (e) => toast.error(e.message),
                });
                setPendingDelete(null);
              }}
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
