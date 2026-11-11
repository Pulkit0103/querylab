"use client";

import { UserRound } from "lucide-react";

import { cn } from "@/lib/utils";

/** Account area of the sidebar. Becomes an account menu once auth exists. */
export function UserMenu({ collapsed }: { collapsed: boolean }) {
  return (
    <div className={cn("flex min-w-0 items-center gap-2 px-1", collapsed && "px-0")}>
      <div className="flex size-7 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground">
        <UserRound className="size-3.5" />
      </div>
      {!collapsed && <span className="truncate text-xs text-muted-foreground">Local workspace</span>}
    </div>
  );
}
