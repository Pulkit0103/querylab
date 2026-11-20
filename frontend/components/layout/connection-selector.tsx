"use client";

import { Fragment } from "react";
import { Database, Plus, Settings2, ShieldCheck } from "lucide-react";
import Link from "next/link";

import { EnvironmentBadge, StatusDot } from "@/components/connections/connection-list";
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
import { useActiveConnection, useConnectionGroups } from "@/hooks/use-connections";
import { groupConnections } from "@/lib/connection-meta";
import { cn } from "@/lib/utils";
import { useWorkspaceStore } from "@/stores/workspace-store";

/** Active database selector shown in the top bar. */
export function ConnectionSelector() {
  const { connection, connections, isPending } = useActiveConnection();
  const { data: groups = [] } = useConnectionGroups();
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

  const sections = groupConnections(connections, groups).filter((s) => s.connections.length > 0);
  const production = connection?.environment === "production";

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="outline"
            size="sm"
            className={cn("h-7 max-w-64 gap-1.5 text-xs", production && "border-destructive/50")}
            aria-label="Active database"
          />
        }
      >
        {connection ? <StatusDot connection={connection} className="size-3" /> : <Database className="size-3.5 shrink-0" />}
        {connection?.color && (
          <span aria-hidden className="size-2 shrink-0 rounded-sm" style={{ background: connection.color }} />
        )}
        <span className="truncate">{connection?.name ?? "Select database"}</span>
        {connection && <EnvironmentBadge environment={connection.environment} />}
        {connection?.read_only && <ShieldCheck className="size-3 shrink-0 text-success" aria-label="Read-only" />}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-[70vh] w-72 overflow-y-auto">
        <DropdownMenuRadioGroup value={connection?.id ?? ""} onValueChange={(v) => setActive(String(v))}>
          {sections.map(({ group, connections: items }, i) => (
            <Fragment key={group?.id ?? "ungrouped"}>
              {i > 0 && <DropdownMenuSeparator />}
              <DropdownMenuGroup>
                <DropdownMenuLabel>{group ? group.name : sections.length > 1 ? "Ungrouped" : "Databases"}</DropdownMenuLabel>
                {items.map((c) => (
                  <DropdownMenuRadioItem key={c.id} value={c.id} className="text-xs">
                    <span className="flex min-w-0 flex-1 items-center gap-2">
                      <StatusDot connection={c} className="size-3" />
                      <span className="flex min-w-0 flex-col">
                        <span className="flex items-center gap-1.5 truncate">
                          {c.color && <span aria-hidden className="size-2 shrink-0 rounded-sm" style={{ background: c.color }} />}
                          <span className="truncate">{c.name}</span>
                          <EnvironmentBadge environment={c.environment} />
                        </span>
                        <span className="truncate font-mono text-[10px] text-muted-foreground">
                          {c.username}@{c.host}/{c.database}
                        </span>
                      </span>
                    </span>
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuGroup>
            </Fragment>
          ))}
        </DropdownMenuRadioGroup>
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
