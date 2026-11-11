"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { PanelLeftClose, PanelLeftOpen, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { useWorkspaceStore } from "@/stores/workspace-store";

import { Logo } from "./logo";
import { navItems } from "./nav-items";
import { ThemeToggle } from "./theme-toggle";
import { UserMenu } from "./user-menu";

function NavLinks({ collapsed, onNavigate }: { collapsed: boolean; onNavigate?: () => void }) {
  const pathname = usePathname();
  return (
    <nav className="flex flex-col gap-0.5 px-2" aria-label="Main">
      {navItems.map((item) => {
        const active = pathname === item.href || pathname.startsWith(`${item.href}/`);
        const link = (
          <Link
            href={item.href}
            onClick={onNavigate}
            aria-current={active ? "page" : undefined}
            className={cn(
              "group flex h-8 items-center gap-2.5 rounded-md px-2 text-[13px] text-sidebar-foreground/70 transition-colors",
              "hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
              active && "bg-sidebar-accent font-medium text-sidebar-accent-foreground",
              collapsed && "justify-center px-0",
            )}
          >
            <item.icon
              className={cn("size-4 shrink-0", active ? "text-brand" : "text-sidebar-foreground/55")}
            />
            {!collapsed && <span className="truncate">{item.label}</span>}
          </Link>
        );
        if (!collapsed) return <div key={item.href}>{link}</div>;
        return (
          <Tooltip key={item.href}>
            <TooltipTrigger render={<div />}>{link}</TooltipTrigger>
            <TooltipContent side="right">{item.label}</TooltipContent>
          </Tooltip>
        );
      })}
    </nav>
  );
}

export function AppSidebar() {
  const collapsed = useWorkspaceStore((s) => s.sidebarCollapsed);
  const toggle = useWorkspaceStore((s) => s.toggleSidebar);
  const mobileOpen = useWorkspaceStore((s) => s.mobileNavOpen);
  const setMobileOpen = useWorkspaceStore((s) => s.setMobileNavOpen);

  return (
    <>
      {/* Desktop */}
      <aside
        className={cn(
          "hidden shrink-0 flex-col border-r border-sidebar-border bg-sidebar transition-[width] duration-200 md:flex",
          collapsed ? "w-13" : "w-56",
        )}
      >
        <div className={cn("flex h-12 items-center px-3", collapsed && "justify-center px-0")}>
          <Link href="/editor" aria-label="QueryLab home">
            <Logo collapsed={collapsed} />
          </Link>
        </div>
        <div className="flex-1 overflow-y-auto py-2 scrollbar-thin">
          <NavLinks collapsed={collapsed} />
        </div>
        <div
          className={cn(
            "flex items-center gap-1 border-t border-sidebar-border p-2",
            collapsed ? "flex-col" : "justify-between",
          )}
        >
          <UserMenu collapsed={collapsed} />
          <div className={cn("flex items-center gap-0.5", collapsed && "flex-col")}>
            <ThemeToggle />
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={toggle}
              aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            >
              {collapsed ? <PanelLeftOpen className="size-4" /> : <PanelLeftClose className="size-4" />}
            </Button>
          </div>
        </div>
      </aside>

      {/* Mobile drawer */}
      {mobileOpen && (
        <div className="fixed inset-0 z-50 md:hidden" role="dialog" aria-modal="true" aria-label="Navigation">
          <button
            type="button"
            aria-label="Close navigation"
            className="absolute inset-0 bg-black/50 animate-in fade-in-0"
            onClick={() => setMobileOpen(false)}
          />
          <aside className="absolute inset-y-0 left-0 flex w-64 flex-col border-r border-sidebar-border bg-sidebar animate-in slide-in-from-left">
            <div className="flex h-12 items-center justify-between px-3">
              <Logo />
              <Button variant="ghost" size="icon-sm" onClick={() => setMobileOpen(false)} aria-label="Close">
                <X className="size-4" />
              </Button>
            </div>
            <div className="flex-1 overflow-y-auto py-2">
              <NavLinks collapsed={false} onNavigate={() => setMobileOpen(false)} />
            </div>
            <div className="flex items-center justify-between border-t border-sidebar-border p-2">
              <UserMenu collapsed={false} />
              <ThemeToggle />
            </div>
          </aside>
        </div>
      )}
    </>
  );
}
