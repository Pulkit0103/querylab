"use client";

import { Database, Plus, Settings2, ShieldCheck } from "lucide-react";
import Link from "next/link";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Skeleton } from "@/components/ui/skeleton";
import { useActiveConnection } from "@/hooks/use-connections";
import { useWorkspaceStore } from "@/stores/workspace-store";

/** Active database selector shown in the top bar. */
export function ConnectionSelector() {
  const { connection, connections, isPending } = useActiveConnection();
  const setActive = useWorkspaceStore((s) => s.setActiveConnection);
  const openDialog = useWorkspaceStore((s) => s.openConnectionDialog);

  if (isPending) return <Skeleton className="h-7 w-40" />;

  if (connections.length === 0) {
    return (
      <Button size="sm" variant="outline" className="h-7 gap-1.5 text-xs" onClick={() => openDialog()}>
        <Plus className="size-3.5" /> Add database
      </Button>
    );
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button variant="outline" size="sm" className="h-7 max-w-56 gap-1.5 text-xs" aria-label="Active database" />
        }
      >
        <Database className="size-3.5 shrink-0 text-brand" />
        <span className="truncate">{connection?.name ?? "Select database"}</span>
        {connection?.read_only && <ShieldCheck className="size-3 shrink-0 text-success" aria-label="Read-only" />}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-64">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Databases</DropdownMenuLabel>
          <DropdownMenuRadioGroup value={connection?.id ?? ""} onValueChange={(v) => setActive(String(v))}>
            {connections.map((c) => (
              <DropdownMenuRadioItem key={c.id} value={c.id} className="text-xs">
                <span className="flex min-w-0 flex-col">
                  <span className="truncate">{c.name}</span>
                  <span className="truncate font-mono text-[10px] text-muted-foreground">
                    {c.username}@{c.host}/{c.database}
                  </span>
                </span>
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => openDialog()} className="text-xs">
          <Plus /> Add connection…
        </DropdownMenuItem>
        <DropdownMenuItem render={<Link href="/settings#connections" />} className="text-xs">
          <Settings2 /> Manage connections
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
