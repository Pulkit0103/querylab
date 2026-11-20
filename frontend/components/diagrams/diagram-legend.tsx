"use client";

import { useState } from "react";
import { ChevronDown, Fingerprint, KeyRound, Link2 } from "lucide-react";

import { endGlyph, type EndKind } from "@/lib/diagram";
import { cn } from "@/lib/utils";

const ENDS: { kind: EndKind; label: string }[] = [
  { kind: "one", label: "Exactly one" },
  { kind: "zero_or_one", label: "Zero or one" },
  { kind: "zero_or_many", label: "Zero or many" },
];

function EndSample({ kind }: { kind: EndKind }) {
  // The node edge is on the left of the sample, the line runs to the right.
  return (
    <svg width="44" height="16" viewBox="0 0 44 16" aria-hidden className="shrink-0 stroke-muted-foreground" fill="none">
      <line x1="0" y1="2" x2="0" y2="14" className="stroke-border" strokeWidth={2} />
      <line x1="0" y1="8" x2="44" y2="8" strokeWidth={1.25} />
      {endGlyph(kind, 0, 8, 1).map((g, i) =>
        g.type === "line" ? (
          <line key={i} x1={g.x1} y1={g.y1} x2={g.x2} y2={g.y2} strokeWidth={1.25} />
        ) : (
          <circle key={i} cx={g.cx} cy={g.cy} r={g.r} strokeWidth={1.25} className="fill-card" />
        ),
      )}
    </svg>
  );
}

/** Explains the relationship markers and key icons. */
export function DiagramLegend() {
  const [open, setOpen] = useState(true);
  return (
    <div className="rounded-lg border bg-card text-xs">
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-1.5 font-medium"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        Legend
        <ChevronDown className={cn("ml-auto size-3.5 transition-transform", !open && "-rotate-90")} />
      </button>
      {open && (
        <div className="space-y-1.5 border-t px-3 py-2">
          {ENDS.map((e) => (
            <div key={e.kind} className="flex items-center gap-2">
              <EndSample kind={e.kind} />
              <span>{e.label}</span>
            </div>
          ))}
          <div className="flex flex-wrap gap-x-3 gap-y-1 border-t pt-1.5 text-muted-foreground">
            <span className="flex items-center gap-1">
              <KeyRound className="size-3 text-warning" /> Primary key
            </span>
            <span className="flex items-center gap-1">
              <Link2 className="size-3 text-chart-2" /> Foreign key
            </span>
            <span className="flex items-center gap-1">
              <Fingerprint className="size-3" /> Unique
            </span>
          </div>
        </div>
      )}
    </div>
  );
}
