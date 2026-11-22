"use client";

import { useState } from "react";
import { Check, ChevronsUpDown, Plus, User, Users } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { useActiveWorkspace, useCreateWorkspace } from "@/hooks/use-workspaces";
import { cn } from "@/lib/utils";
import { useWorkspaceStore } from "@/stores/workspace-store";

function NewWorkspaceDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const create = useCreateWorkspace();
  const [name, setName] = useState("");
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>New team workspace</DialogTitle>
          <DialogDescription>
            Share connections, saved queries and snippets with your team. You can invite people once it is created.
          </DialogDescription>
        </DialogHeader>
        <form
          id="new-workspace"
          onSubmit={(e) => {
            e.preventDefault();
            create.mutate(name.trim(), {
              onSuccess: (ws) => {
                toast.success(`Switched to ${ws.name}`);
                setName("");
                onClose();
              },
              onError: (err) => toast.error(err.message),
            });
          }}
        >
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Workspace name" aria-label="Workspace name" maxLength={100} autoFocus />
        </form>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" form="new-workspace" disabled={!name.trim() || create.isPending}>
            Create
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** Chooses the workspace whose connections, queries and snippets are shown. */
export function WorkspaceSwitcher({ collapsed }: { collapsed: boolean }) {
  const { workspace, workspaces, isPending } = useActiveWorkspace();
  const setActive = useWorkspaceStore((s) => s.setActiveWorkspace);
  const [creating, setCreating] = useState(false);

  if (isPending) return <Skeleton className={cn("h-8", collapsed ? "mx-auto w-8" : "mx-2")} />;
  if (!workspace) return null;
  const Icon = workspace.personal ? User : Users;

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <button
              type="button"
              aria-label={`Workspace: ${workspace.name}`}
              className={cn(
                "flex h-8 w-full min-w-0 items-center gap-2 rounded-md border border-sidebar-border px-2 text-left text-xs outline-none hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-ring",
                collapsed && "justify-center px-0",
              )}
            />
          }
        >
          <Icon className="size-3.5 shrink-0 text-brand" />
          {!collapsed && (
            <>
              <span className="min-w-0 flex-1 truncate font-medium">{workspace.name}</span>
              <ChevronsUpDown className="size-3 shrink-0 text-muted-foreground" />
            </>
          )}
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-60">
          <DropdownMenuLabel className="text-[11px] font-normal text-muted-foreground">Workspaces</DropdownMenuLabel>
          {workspaces.map((w) => (
            <DropdownMenuItem key={w.id} onClick={() => setActive(w.id)}>
              {w.personal ? <User /> : <Users />}
              <span className="min-w-0 flex-1 truncate">{w.name}</span>
              <span className="text-[10px] text-muted-foreground">{w.personal ? "personal" : w.role}</span>
              {w.id === workspace.id && <Check className="text-brand" />}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={() => setCreating(true)}>
            <Plus /> New team workspace
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <NewWorkspaceDialog open={creating} onClose={() => setCreating(false)} />
    </>
  );
}
