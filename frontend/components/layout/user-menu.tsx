"use client";

import Link from "next/link";
import { LogOut, Settings } from "lucide-react";

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Skeleton } from "@/components/ui/skeleton";
import { useLogout, useMe } from "@/hooks/use-auth";
import { cn } from "@/lib/utils";

function initials(name: string): string {
  const parts = name.split(/[\s@._-]+/).filter(Boolean);
  return (parts[0]?.[0] ?? "?").concat(parts[1]?.[0] ?? "").toUpperCase();
}

export function UserMenu({ collapsed }: { collapsed: boolean }) {
  const { data: user, isPending } = useMe();
  const logout = useLogout();

  if (isPending) return <Skeleton className="size-7 rounded-full" />;
  if (!user) return null;

  const label = user.display_name || user.email;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <button
            type="button"
            className={cn(
              "flex min-w-0 items-center gap-2 rounded-md p-1 text-left outline-none hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-ring",
              !collapsed && "pr-2",
            )}
            aria-label="Account menu"
          />
        }
      >
        <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-brand/15 text-[10px] font-semibold text-brand">
          {initials(label)}
        </span>
        {!collapsed && <span className="truncate text-xs text-sidebar-foreground/80">{label}</span>}
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="start" className="w-56">
        <DropdownMenuGroup>
          <DropdownMenuLabel className="truncate">
            <span className="block truncate text-xs font-medium text-foreground">{label}</span>
            {user.display_name && <span className="block truncate text-xs font-normal">{user.email}</span>}
          </DropdownMenuLabel>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem render={<Link href="/settings" />}>
          <Settings /> Settings
        </DropdownMenuItem>
        <DropdownMenuItem variant="destructive" onClick={() => logout.mutate()} disabled={logout.isPending}>
          <LogOut /> Sign out
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
