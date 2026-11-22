"use client";

import { useState } from "react";
import { ArrowLeftRight, GitCompareArrows, Loader2 } from "lucide-react";

import { EmptyState } from "@/components/common/empty-state";
import { DiffView } from "@/components/migrations/diff-view";
import { SidePicker } from "@/components/migrations/side-picker";
import { Button } from "@/components/ui/button";
import { useConnections } from "@/hooks/use-connections";
import { useSchemaDiff } from "@/hooks/use-migrations";

interface Side {
  connectionId: string | null;
  schema: string;
}

export default function SchemaDiffPage() {
  const { data: connections } = useConnections();
  const [source, setSource] = useState<Side>({ connectionId: null, schema: "" });
  const [target, setTarget] = useState<Side>({ connectionId: null, schema: "" });
  const diff = useSchemaDiff();
  const ready = source.connectionId && source.schema && target.connectionId && target.schema.trim();
  const name = (s: Side) => `${connections?.find((c) => c.id === s.connectionId)?.name ?? "?"}.${s.schema}`;

  const compare = () => {
    if (!ready) return;
    diff.mutate({
      source: { connection_id: source.connectionId!, schema: source.schema },
      target: { connection_id: target.connectionId!, schema: target.schema.trim() },
    });
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 flex-wrap items-end gap-4 border-b bg-panel px-3 py-3">
        <SidePicker label="Source (desired state)" connectionId={source.connectionId} schema={source.schema} onChange={setSource} />
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Swap source and target"
          onClick={() => {
            setSource(target);
            setTarget(source);
          }}
        >
          <ArrowLeftRight className="size-4" />
        </Button>
        <SidePicker label="Target (to change)" connectionId={target.connectionId} schema={target.schema} onChange={setTarget} allowNewSchema />
        <Button onClick={compare} disabled={!ready || diff.isPending} className="gap-1.5">
          {diff.isPending ? <Loader2 className="size-4 animate-spin" /> : <GitCompareArrows className="size-4" />} Compare
        </Button>
      </div>
      {diff.isError ? (
        <p className="p-4 text-sm text-destructive">{diff.error.message}</p>
      ) : diff.data ? (
        <DiffView key={diff.submittedAt} diff={diff.data} sourceLabel={name(source)} />
      ) : (
        <EmptyState
          icon={GitCompareArrows}
          title="Compare two schemas"
          description="Pick the schema with the desired structure (for example development) and the one to update (for example staging). QueryLab lists every difference and writes the SQL to apply it."
        />
      )}
    </div>
  );
}
