"use client";

import { useState } from "react";
import { ChevronDown, Gauge, GitBranch, ShieldCheck } from "lucide-react";

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
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

type Target = { sql: string; offset: number };

interface ExplainMenuProps {
  disabled: boolean;
  /** Resolves the statement to explain (selection or statement under cursor). */
  getTarget: () => Target;
  onExplain: (target: Target, analyze: boolean) => void;
}

const PREVIEW_CHARS = 400;

/**
 * Dropdown next to the Explain button offering EXPLAIN ANALYZE. Because
 * ANALYZE really executes the statement, it always goes through an explicit
 * confirmation unless the user opted out for the current browser session.
 */
export function ExplainMenu({ disabled, getTarget, onExplain }: ExplainMenuProps) {
  const [pending, setPending] = useState<Target | null>(null);
  const [skipConfirm, setSkipConfirm] = useState(false);
  const [dontAsk, setDontAsk] = useState(false);

  const requestAnalyze = () => {
    const target = getTarget();
    if (!target.sql.trim()) return;
    if (skipConfirm) onExplain(target, true);
    else {
      setDontAsk(false);
      setPending(target);
    }
  };

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="sm"
              disabled={disabled}
              aria-label="More explain options"
              className="-ml-1 px-1"
            />
          }
        >
          <ChevronDown className="size-3.5" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-60">
          <DropdownMenuItem onClick={() => onExplain(getTarget(), false)}>
            <GitBranch /> Explain
            <span className="ml-auto text-[10px] text-muted-foreground">estimates only</span>
          </DropdownMenuItem>
          <DropdownMenuItem onClick={requestAnalyze}>
            <Gauge /> Explain Analyze…
            <span className="ml-auto text-[10px] text-muted-foreground">runs the query</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <AlertDialog open={pending !== null} onOpenChange={(o) => !o && setPending(null)}>
        <AlertDialogContent className="sm:max-w-lg">
          <AlertDialogHeader>
            <AlertDialogTitle>Run EXPLAIN ANALYZE?</AlertDialogTitle>
            <AlertDialogDescription>
              EXPLAIN ANALYZE executes the statement to measure real row counts and timings. It can take as long as the
              query itself and uses database resources.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <pre className="max-h-40 overflow-auto rounded-md border bg-muted/50 p-2 font-mono text-xs whitespace-pre-wrap">
            {pending && pending.sql.length > PREVIEW_CHARS ? `${pending.sql.slice(0, PREVIEW_CHARS)}…` : pending?.sql}
          </pre>
          <p className="flex gap-2 text-xs text-muted-foreground">
            <ShieldCheck className="size-4 shrink-0 text-success" />
            It runs inside a read-only transaction that is always rolled back, so statements that modify data fail
            instead of changing anything. The statement timeout still applies.
          </p>
          <label className="flex items-center gap-2 text-xs">
            <Checkbox checked={dontAsk} onCheckedChange={(v) => setDontAsk(v === true)} />
            Don&apos;t ask again until I reload the page
          </label>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (!pending) return;
                if (dontAsk) setSkipConfirm(true);
                onExplain(pending, true);
                setPending(null);
              }}
            >
              <Gauge className="size-3.5" /> Run Explain Analyze
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
