"use client";

import { useMemo, useState } from "react";
import { AlertTriangle, ArrowRight, CheckCircle2, Copy, Lightbulb, Minus, Pencil, Plus, Save } from "lucide-react";
import { toast } from "sonner";

import { SaveMigrationDialog } from "@/components/migrations/save-migration-dialog";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { buildDownScript, buildScript, defaultSelection, groupChanges, isManual, migrationName, summarize } from "@/lib/schema-diff";
import { cn } from "@/lib/utils";
import type { DiffResponse, SchemaChange } from "@/types/migrations";

const ACTION_ICON = { create: Plus, alter: Pencil, drop: Minus } as const;
const ACTION_CLASS = { create: "text-success", alter: "text-warning", drop: "text-destructive" } as const;

function Definition({ title, text, tone }: { title: string; text: string | null; tone: "before" | "after" }) {
  return (
    <div className="min-w-0 flex-1">
      <p className="mb-1 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">{title}</p>
      <pre
        className={cn(
          "max-h-64 overflow-auto rounded-md border p-2 font-mono text-[11px] whitespace-pre-wrap scrollbar-thin",
          text === null && "text-muted-foreground italic",
          tone === "before" ? "bg-destructive/5" : "bg-success/5",
        )}
      >
        {text ?? (tone === "before" ? "(does not exist)" : "(removed)")}
      </pre>
    </div>
  );
}

function ChangeDetail({ change }: { change: SchemaChange }) {
  return (
    <div className="space-y-3 text-xs">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-mono text-sm font-medium">{change.object}</span>
        <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] uppercase">
          {change.action} {change.object_type}
        </span>
        {change.destructive && (
          <span className="flex items-center gap-1 rounded bg-destructive/15 px-1.5 py-0.5 text-[10px] font-medium text-destructive">
            <AlertTriangle className="size-3" /> destructive
          </span>
        )}
      </div>
      {change.notes.length > 0 && (
        <ul className="space-y-1">
          {change.notes.map((n) => (
            <li key={n} className="flex gap-1.5 text-warning">
              <AlertTriangle className="mt-0.5 size-3.5 shrink-0" /> {n}
            </li>
          ))}
        </ul>
      )}
      <div className="flex flex-col gap-3 md:flex-row">
        <Definition title="Target now" text={change.before} tone="before" />
        <Definition title="After migration (source)" text={change.after} tone="after" />
      </div>
      <div>
        <p className="mb-1 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">SQL</p>
        <pre className="overflow-auto rounded-md border bg-panel p-2 font-mono text-[11px] whitespace-pre-wrap">{change.sql}</pre>
      </div>
      {change.down_sql && (
        <div>
          <p className="mb-1 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">Undo</p>
          <pre className="overflow-auto rounded-md border bg-panel p-2 font-mono text-[11px] whitespace-pre-wrap text-muted-foreground">
            {change.down_sql}
          </pre>
        </div>
      )}
    </div>
  );
}

/** Differences as a tree with checkboxes, the selected change, and the script. */
export function DiffView({ diff, sourceLabel }: { diff: DiffResponse; sourceLabel: string }) {
  const [selected, setSelected] = useState(() => defaultSelection(diff.changes));
  const [focus, setFocus] = useState(0);
  const [tab, setTab] = useState<"change" | "script">("change");
  const [saving, setSaving] = useState(false);
  const groups = useMemo(() => groupChanges(diff.changes), [diff.changes]);
  const counts = summarize(diff.changes);
  const script = buildScript(diff, selected);
  const down = buildDownScript(diff, selected);
  const destructiveSelected = diff.changes.filter((c, i) => c.destructive && selected.has(i)).length;

  if (diff.changes.length === 0) {
    return (
      <div className="flex flex-col items-center gap-2 py-16 text-center text-sm text-muted-foreground">
        <CheckCircle2 className="size-7 text-success" />
        The schemas match: there is nothing to migrate.
      </div>
    );
  }

  const toggle = (indices: number[], on: boolean) =>
    setSelected((s) => {
      const next = new Set(s);
      for (const i of indices) {
        if (on && !isManual(diff.changes[i])) next.add(i);
        else next.delete(i);
      }
      return next;
    });

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b px-3 py-2 text-xs">
        <span>
          <span className="font-medium">{diff.changes.length}</span> differences:{" "}
          <span className="text-success">{counts.create} to create</span> ·{" "}
          <span className="text-warning">{counts.alter} to change</span> ·{" "}
          <span className="text-destructive">{counts.drop} to drop</span>
        </span>
        {counts.destructive > 0 && (
          <span className="text-muted-foreground">
            Destructive changes are not selected by default ({destructiveSelected} of {counts.destructive} selected).
          </span>
        )}
        <div className="ml-auto flex gap-1">
          <Button variant={tab === "change" ? "secondary" : "ghost"} size="xs" onClick={() => setTab("change")}>
            Selected change
          </Button>
          <Button variant={tab === "script" ? "secondary" : "ghost"} size="xs" onClick={() => setTab("script")}>
            Script ({selected.size})
          </Button>
          <Button size="xs" className="gap-1" disabled={selected.size === 0} onClick={() => setSaving(true)}>
            <Save className="size-3.5" /> Save as migration
          </Button>
        </div>
      </div>
      {diff.hints.length > 0 && (
        <ul className="shrink-0 space-y-1 border-b bg-brand/5 px-3 py-2 text-xs">
          {diff.hints.map((h) => (
            <li key={h} className="flex gap-1.5">
              <Lightbulb className="mt-0.5 size-3.5 shrink-0 text-brand" /> {h}
            </li>
          ))}
        </ul>
      )}
      <div className="flex min-h-0 flex-1 flex-col md:flex-row">
        <nav className="max-h-72 shrink-0 overflow-auto border-b scrollbar-thin md:max-h-none md:w-80 md:border-r md:border-b-0" aria-label="Differences">
          {groups.map((g) => {
            const all = g.items.filter((i) => !isManual(diff.changes[i]));
            const checked = all.length > 0 && all.every((i) => selected.has(i));
            return (
              <section key={g.type}>
                <label className="flex items-center gap-2 bg-panel px-3 py-1.5 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
                  <Checkbox checked={checked} disabled={all.length === 0} onCheckedChange={(v) => toggle(all, v === true)} aria-label={`Select all ${g.label}`} />
                  {g.label}
                  <span className="ml-auto tabular-nums">{g.items.length}</span>
                </label>
                <ul>
                  {g.items.map((i) => {
                    const c = diff.changes[i];
                    const Icon = ACTION_ICON[c.action];
                    return (
                      <li key={i} className={cn("flex items-center gap-2 px-3 py-1 text-xs", focus === i && "bg-accent/60")}>
                        <Checkbox
                          checked={selected.has(i)}
                          disabled={isManual(c)}
                          onCheckedChange={(v) => toggle([i], v === true)}
                          aria-label={`Include ${c.action} ${c.object}`}
                        />
                        <button
                          type="button"
                          className="flex min-w-0 flex-1 items-center gap-1.5 text-left"
                          onClick={() => {
                            setFocus(i);
                            setTab("change");
                          }}
                        >
                          <Icon className={cn("size-3.5 shrink-0", ACTION_CLASS[c.action])} aria-label={c.action} />
                          <span className="truncate font-mono">{c.object}</span>
                          {c.destructive && <AlertTriangle className="size-3 shrink-0 text-destructive" aria-label="destructive" />}
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </section>
            );
          })}
        </nav>
        <div className="min-h-0 min-w-0 flex-1 overflow-auto p-3 scrollbar-thin">
          {tab === "change" ? (
            <ChangeDetail change={diff.changes[focus] ?? diff.changes[0]} />
          ) : (
            <div className="space-y-2">
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <span>
                  Runs as one transaction. <span className="font-mono">{diff.target_schema}</span> <ArrowRight className="inline size-3" /> matches{" "}
                  <span className="font-mono">{sourceLabel}</span> for the selected changes.
                </span>
                <Button
                  variant="ghost"
                  size="xs"
                  className="ml-auto gap-1"
                  disabled={!script}
                  onClick={() =>
                    void navigator.clipboard.writeText(script).then(
                      () => toast.success("Script copied"),
                      () => toast.error("Clipboard access was denied by the browser."),
                    )
                  }
                >
                  <Copy className="size-3.5" /> Copy
                </Button>
              </div>
              <pre className="overflow-auto rounded-md border bg-panel p-3 font-mono text-[11px] whitespace-pre-wrap" aria-label="Migration script">
                {script || "-- Select changes to include them in the script."}
              </pre>
            </div>
          )}
        </div>
      </div>
      <SaveMigrationDialog
        open={saving}
        onOpenChange={setSaving}
        defaultName={migrationName(diff, sourceLabel)}
        upSql={script}
        downSql={down.sql}
        downMissing={down.missing}
      />
    </div>
  );
}
