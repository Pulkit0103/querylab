"use client";

import type { ReactNode } from "react";
import { usePathname } from "next/navigation";
import { Menu } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useWorkspaceStore } from "@/stores/workspace-store";

import { ApiStatus } from "./api-status";
import { navItems } from "./nav-items";

export function TopBar({ actions }: { actions?: ReactNode }) {
  const pathname = usePathname();
  const setMobileOpen = useWorkspaceStore((s) => s.setMobileNavOpen);
  const current = navItems.find((i) => pathname === i.href || pathname.startsWith(`${i.href}/`));

  return (
    <header className="flex h-12 shrink-0 items-center gap-2 border-b bg-background px-3">
      <Button
        variant="ghost"
        size="icon-sm"
        className="md:hidden"
        onClick={() => setMobileOpen(true)}
        aria-label="Open navigation"
      >
        <Menu className="size-4" />
      </Button>
      <div className="flex min-w-0 items-center gap-2">
        {current && <current.icon className="size-4 text-muted-foreground" />}
        <h1 className="truncate text-sm font-medium">{current?.label ?? "QueryLab"}</h1>
      </div>
      <div className="ml-auto flex items-center gap-2">
        {actions}
        <ApiStatus />
      </div>
    </header>
  );
}
