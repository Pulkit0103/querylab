"use client";

import { useEffect, useState, type ReactNode } from "react";
import {
  CheckCircle2,
  Copy,
  Eraser,
  GitBranch,
  Loader2,
  Play,
  Save,
  Square,
  Wand2,
  XCircle,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatDuration, formatNumber } from "@/lib/format";
import type { ExecState } from "@/stores/editor-store";

interface EditorToolbarProps {
  exec: ExecState;
  canRun: boolean;
  onRun: () => void;
  onCancel: () => void;
  onExplain: () => void;
  onFormat: () => void;
  onSave: () => void;
  onCopy: () => void;
  onClear: () => void;
  /** Extra controls rendered next to Explain (e.g. EXPLAIN ANALYZE menu). */
  explainExtra?: ReactNode;
}

function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="ml-1 rounded border border-background/20 px-1 font-mono text-[10px] opacity-70">
      {children}
    </kbd>
  );
}

function ToolButton({
  label,
  shortcut,
  onClick,
  disabled,
  children,
}: {
  label: string;
  shortcut?: string;
  onClick: () => void;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button variant="ghost" size="sm" onClick={onClick} disabled={disabled} aria-label={label} />
        }
      >
        {children}
      </TooltipTrigger>
      <TooltipContent side="bottom">
        {label}
        {shortcut && <Kbd>{shortcut}</Kbd>}
      </TooltipContent>
    </Tooltip>
  );
}

function ElapsedTimer({ startedAt }: { startedAt: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 100);
    return () => clearInterval(t);
  }, []);
  return <span className="tabular-nums">{formatDuration(now - startedAt)}</span>;
}

function ExecStatus({ exec }: { exec: ExecState }) {
  switch (exec.status) {
    case "idle":
      return <span className="text-muted-foreground">Ready</span>;
    case "running":
      return (
        <span className="flex items-center gap-1.5 text-brand">
          <Loader2 className="size-3.5 animate-spin" />
          {exec.kind === "explain" ? "Explaining" : "Running"} · <ElapsedTimer startedAt={exec.startedAt} />
        </span>
      );
    case "success":
      return (
        <span className="flex items-center gap-1.5 text-muted-foreground">
          <CheckCircle2 className="size-3.5 text-success" />
          {exec.result.rows_affected !== null && exec.result.columns.length === 0
            ? `${formatNumber(exec.result.rows_affected)} rows affected`
            : `${formatNumber(exec.result.row_count)} rows`}
          <span className="text-muted-foreground/60">·</span>
          <span className="tabular-nums">{formatDuration(exec.result.execution_time_ms)}</span>
        </span>
      );
    case "error":
      return (
        <span className="flex items-center gap-1.5 text-destructive">
          <XCircle className="size-3.5" /> Failed
        </span>
      );
  }
}

export function EditorToolbar(props: EditorToolbarProps) {
  const running = props.exec.status === "running";
  return (
    <div className="flex h-10 shrink-0 items-center gap-1 border-b px-2">
      {running ? (
        <Button size="sm" variant="destructive" onClick={props.onCancel} className="gap-1.5">
          <Square className="size-3 fill-current" /> Cancel
        </Button>
      ) : (
        <Tooltip>
          <TooltipTrigger
            render={
              <Button
                size="sm"
                onClick={props.onRun}
                disabled={!props.canRun}
                className="gap-1.5 bg-brand text-brand-foreground hover:bg-brand/85"
              />
            }
          >
            <Play className="size-3 fill-current" /> Run
          </TooltipTrigger>
          <TooltipContent side="bottom">
            Run selection or statement <Kbd>Ctrl ↵</Kbd>
          </TooltipContent>
        </Tooltip>
      )}

      <ToolButton label="Explain" onClick={props.onExplain} disabled={running || !props.canRun}>
        <GitBranch className="size-3.5" />
        <span className="hidden sm:inline">Explain</span>
      </ToolButton>
      {props.explainExtra}

      <Separator orientation="vertical" className="mx-1 h-5" />

      <ToolButton label="Format SQL" shortcut="Ctrl ⇧ F" onClick={props.onFormat}>
        <Wand2 className="size-3.5" />
        <span className="hidden lg:inline">Format</span>
      </ToolButton>
      <ToolButton label="Save query" shortcut="Ctrl S" onClick={props.onSave}>
        <Save className="size-3.5" />
        <span className="hidden lg:inline">Save</span>
      </ToolButton>
      <ToolButton label="Copy query" onClick={props.onCopy}>
        <Copy className="size-3.5" />
      </ToolButton>
      <ToolButton label="Clear editor" onClick={props.onClear}>
        <Eraser className="size-3.5" />
      </ToolButton>

      <div className="ml-auto truncate pl-2 text-xs">
        <ExecStatus exec={props.exec} />
      </div>
    </div>
  );
}
