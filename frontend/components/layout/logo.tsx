import { cn } from "@/lib/utils";

export function Logo({ collapsed = false, className }: { collapsed?: boolean; className?: string }) {
  return (
    <div className={cn("flex items-center gap-2", className)}>
      <div className="flex size-7 shrink-0 items-center justify-center rounded-md bg-brand text-brand-foreground">
        <svg viewBox="0 0 24 24" className="size-4" fill="none" stroke="currentColor" strokeWidth={2.25} aria-hidden>
          <ellipse cx="12" cy="6" rx="7" ry="3" />
          <path d="M5 6v6c0 1.66 3.13 3 7 3" />
          <path d="M5 12v6c0 1.66 3.13 3 7 3" />
          <path d="m15 15 2.5 2.5L15 20M19 20h2" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </div>
      {!collapsed && <span className="text-sm font-semibold tracking-tight">QueryLab</span>}
    </div>
  );
}
